use std::sync::Arc;

use nonproxy_flow_protocol::{
    FlowEndpoint, FlowFrame, FlowId, FrameType, OpenFlowRequest, WindowUpdate, read_frame,
    write_frame,
};
use nonproxy_model::OutboundId;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{OutboundKind, OutboundReference, PolicyDatabase};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UnixStream},
    time::{Duration, timeout},
};
use tokio_stream::wrappers::UnixListenerStream;

use super::{FlowConnectionHandler, FlowServer, FlowWindow};
use crate::{
    Gateway,
    credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
    session_capability::SessionCapability,
};

const CLIENT_WINDOW_BYTES: u32 = 256 * 1024;
const TEST_TIMEOUT: Duration = Duration::from_secs(3);

#[tokio::test]
async fn authenticated_tcp_flow_relays_bytes_and_window_updates() {
    let proxy_listener = bind_proxy_fixture().await;
    let proxy_port = local_port(&proxy_listener);
    let proxy_task = tokio::spawn(http_connect_echo_fixture(proxy_listener));
    let gateway = gateway_with_http_outbound(proxy_port).await;
    let token = [7_u8; 32];
    let handler = handler(gateway, token);
    let (server, mut client) = unix_pair();
    let handler_task = tokio::spawn(async move { handler.handle(server).await });
    let flow_id = flow_id();

    send_open(
        &mut client,
        FrameType::OpenTcp,
        flow_id,
        token,
        "proxy-main",
    )
    .await;
    let initial_window = read_frame_with_timeout(&mut client).await;
    assert_eq!(initial_window.frame_type(), FrameType::WindowUpdate);
    assert_eq!(
        decode_window(&initial_window),
        CLIENT_WINDOW_BYTES,
        "gatewayd 必须显式授予 Provider 入站窗口"
    );

    send_frame(&mut client, FrameType::Data, flow_id, 1, b"hello").await;
    let mut echoed = Vec::new();
    let mut acknowledged = 0_u32;
    while echoed.is_empty() || acknowledged == 0 {
        let frame = read_frame_with_timeout(&mut client).await;
        match frame.frame_type() {
            FrameType::Data => echoed.extend_from_slice(frame.payload()),
            FrameType::WindowUpdate => acknowledged += decode_window(&frame),
            other => panic!("收到非预期 flow 帧: {other:?}"),
        }
    }
    assert_eq!(echoed, b"hello");
    assert_eq!(acknowledged, 5);

    send_frame(&mut client, FrameType::Close, flow_id, 2, &[]).await;
    let terminal = read_until_terminal(&mut client).await;
    assert_eq!(terminal.frame_type(), FrameType::Close);
    await_task(handler_task, "flow handler").await;
    await_task(proxy_task, "HTTP CONNECT fixture").await;
}

#[tokio::test]
async fn rejects_invalid_capability_before_opening_proxy() {
    let gateway = empty_gateway();
    let handler = handler(gateway, [9_u8; 32]);
    let (server, mut client) = unix_pair();
    let handler_task = tokio::spawn(async move { handler.handle(server).await });
    let flow_id = flow_id();

    send_open(
        &mut client,
        FrameType::OpenTcp,
        flow_id,
        [8_u8; 32],
        "missing-proxy",
    )
    .await;
    let error = read_frame_with_timeout(&mut client).await;

    assert_eq!(error.frame_type(), FrameType::Error);
    assert_eq!(error.flow_id(), flow_id);
    assert_eq!(error.payload(), b"NP_FLOW_AUTHENTICATION_FAILED");
    await_task(handler_task, "未授权 flow handler").await;
}

#[tokio::test]
async fn rejects_udp_for_http_connect_without_dialing_proxy() {
    let gateway = gateway_with_http_outbound(9).await;
    let token = [10_u8; 32];
    let handler = handler(gateway, token);
    let (server, mut client) = unix_pair();
    let handler_task = tokio::spawn(async move { handler.handle(server).await });
    let flow_id = flow_id();

    send_open(
        &mut client,
        FrameType::OpenUdp,
        flow_id,
        token,
        "proxy-main",
    )
    .await;
    let error = read_frame_with_timeout(&mut client).await;

    assert_eq!(error.frame_type(), FrameType::Error);
    assert_eq!(error.flow_id(), flow_id);
    assert_eq!(error.payload(), b"NP_FLOW_OUTBOUND_UNSUPPORTED");
    await_task(handler_task, "不支持的 UDP flow handler").await;
}

#[tokio::test]
async fn flow_window_blocks_until_credit_is_added() {
    let window = Arc::new(match FlowWindow::new(16 * 1024) {
        Ok(value) => value,
        Err(error) => panic!("测试窗口创建失败: {error}"),
    });
    if let Err(error) = window.take_exact(16 * 1024).await {
        panic!("测试窗口扣减失败: {error}");
    }
    let mut waiter = tokio::spawn({
        let window = Arc::clone(&window);
        async move { window.take_exact(1).await }
    });
    assert!(
        timeout(Duration::from_millis(20), &mut waiter)
            .await
            .is_err(),
        "窗口耗尽后不应继续发送"
    );
    if let Err(error) = window.add(1).await {
        panic!("测试窗口补充失败: {error}");
    }
    match timeout(TEST_TIMEOUT, waiter).await {
        Ok(Ok(Ok(()))) => {}
        result => panic!("补充窗口后等待任务未完成: {result:?}"),
    }
}

#[tokio::test]
async fn server_drops_connections_above_active_flow_limit() {
    let directory = match tempfile::tempdir() {
        Ok(value) => value,
        Err(error) => panic!("数据面并发测试目录创建失败: {error}"),
    };
    let socket_path = directory.path().join("flow.sock");
    let listener = match tokio::net::UnixListener::bind(&socket_path) {
        Ok(value) => value,
        Err(error) => panic!("数据面并发测试 Socket 绑定失败: {error}"),
    };
    let handler = handler(empty_gateway(), [12_u8; 32]);
    let (shutdown_sender, shutdown_receiver) = tokio::sync::watch::channel(false);
    let flow_server = FlowServer::with_maximum_active_flows(handler, 1);
    let capacity = Arc::clone(&flow_server.capacity);
    let server_task =
        tokio::spawn(flow_server.serve(UnixListenerStream::new(listener), shutdown_receiver));
    let _first = match UnixStream::connect(&socket_path).await {
        Ok(value) => value,
        Err(error) => panic!("第一个数据面测试连接失败: {error}"),
    };
    wait_for_capacity_exhaustion(&capacity).await;
    let mut second = match UnixStream::connect(&socket_path).await {
        Ok(value) => value,
        Err(error) => panic!("第二个数据面测试连接失败: {error}"),
    };
    let mut byte = [0_u8; 1];
    let read = timeout(TEST_TIMEOUT, second.read(&mut byte)).await;
    assert!(matches!(read, Ok(Ok(0))), "超限连接必须立即关闭: {read:?}");

    let _send_result = shutdown_sender.send(true);
    match timeout(TEST_TIMEOUT, server_task).await {
        Ok(Ok(Ok(()))) => {}
        result => panic!("数据面并发测试服务未正常退出: {result:?}"),
    }
}

async fn wait_for_capacity_exhaustion(capacity: &tokio::sync::Semaphore) {
    for _attempt in 0..50 {
        if capacity.available_permits() == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("数据面并发许可未在限定时间内被占用");
}

fn handler(gateway: Gateway, token: [u8; 32]) -> FlowConnectionHandler {
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    FlowConnectionHandler::new(gateway, SessionCapability::from_token(token), store)
}

fn empty_gateway() -> Gateway {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("测试数据库打开失败: {error}"),
    };
    Gateway::new(database, CompileCapabilities::full())
}

async fn gateway_with_http_outbound(port: u16) -> Gateway {
    let gateway = empty_gateway();
    let outbound_id = match OutboundId::new("proxy-main") {
        Ok(value) => value,
        Err(error) => panic!("测试出口 ID 创建失败: {error}"),
    };
    let outbound = match OutboundReference::new(
        outbound_id,
        OutboundKind::HttpConnect,
        Some("127.0.0.1"),
        Some(port),
        None,
        1,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试出口创建失败: {error}"),
    };
    if let Err(error) = gateway.save_outbounds(vec![(outbound, None)]).await {
        panic!("测试出口保存失败: {error}");
    }
    gateway
}

async fn send_open(
    stream: &mut UnixStream,
    frame_type: FrameType,
    flow_id: FlowId,
    token: [u8; 32],
    outbound_id: &str,
) {
    let outbound_id = match OutboundId::new(outbound_id) {
        Ok(value) => value,
        Err(error) => panic!("测试 OPEN 出口 ID 创建失败: {error}"),
    };
    let endpoint = match FlowEndpoint::new("example.test", 443) {
        Ok(value) => value,
        Err(error) => panic!("测试目标创建失败: {error}"),
    };
    let open = match OpenFlowRequest::new(token, outbound_id, endpoint, CLIENT_WINDOW_BYTES) {
        Ok(value) => value,
        Err(error) => panic!("测试 OPEN payload 创建失败: {error}"),
    };
    let payload = match open.encode() {
        Ok(value) => value.to_vec(),
        Err(error) => panic!("测试 OPEN payload 编码失败: {error}"),
    };
    send_frame(stream, frame_type, flow_id, 0, &payload).await;
}

async fn send_frame(
    stream: &mut UnixStream,
    frame_type: FrameType,
    flow_id: FlowId,
    sequence: u64,
    payload: &[u8],
) {
    let frame = match FlowFrame::new(frame_type, 0, flow_id, sequence, payload.to_vec()) {
        Ok(value) => value,
        Err(error) => panic!("测试 flow 帧创建失败: {error}"),
    };
    if let Err(error) = write_frame(stream, &frame).await {
        panic!("测试 flow 帧写入失败: {error}");
    }
}

async fn read_frame_with_timeout(stream: &mut UnixStream) -> FlowFrame {
    match timeout(TEST_TIMEOUT, read_frame(stream)).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => panic!("测试 flow 帧读取失败: {error}"),
        Err(_) => panic!("测试 flow 帧读取超时"),
    }
}

async fn read_until_terminal(stream: &mut UnixStream) -> FlowFrame {
    loop {
        let frame = read_frame_with_timeout(stream).await;
        if matches!(frame.frame_type(), FrameType::Close | FrameType::Error) {
            return frame;
        }
    }
}

fn decode_window(frame: &FlowFrame) -> u32 {
    match WindowUpdate::decode(frame.payload()) {
        Ok(value) => value.bytes(),
        Err(error) => panic!("测试窗口帧解码失败: {error}"),
    }
}

fn unix_pair() -> (UnixStream, UnixStream) {
    match UnixStream::pair() {
        Ok(value) => value,
        Err(error) => panic!("测试 UnixStream pair 创建失败: {error}"),
    }
}

fn flow_id() -> FlowId {
    match FlowId::new([1_u8; 16]) {
        Ok(value) => value,
        Err(error) => panic!("测试 flow ID 创建失败: {error}"),
    }
}

async fn bind_proxy_fixture() -> TcpListener {
    match TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(value) => value,
        Err(error) => panic!("HTTP CONNECT fixture 绑定失败: {error}"),
    }
}

fn local_port(listener: &TcpListener) -> u16 {
    match listener.local_addr() {
        Ok(value) => value.port(),
        Err(error) => panic!("HTTP CONNECT fixture 地址读取失败: {error}"),
    }
}

async fn http_connect_echo_fixture(listener: TcpListener) {
    let (mut stream, _) = match listener.accept().await {
        Ok(value) => value,
        Err(error) => panic!("HTTP CONNECT fixture 接收失败: {error}"),
    };
    let request = read_http_header(&mut stream).await;
    assert!(String::from_utf8_lossy(&request).contains("CONNECT example.test:443 HTTP/1.1"));
    if let Err(error) = stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
    {
        panic!("HTTP CONNECT fixture 响应失败: {error}");
    }
    let mut payload = [0_u8; 5];
    if let Err(error) = stream.read_exact(&mut payload).await {
        panic!("HTTP CONNECT fixture 读取中继数据失败: {error}");
    }
    if let Err(error) = stream.write_all(&payload).await {
        panic!("HTTP CONNECT fixture 回显失败: {error}");
    }
}

async fn read_http_header(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    while request.len() < 16 * 1024 {
        let mut byte = [0_u8; 1];
        if let Err(error) = stream.read_exact(&mut byte).await {
            panic!("HTTP CONNECT fixture 请求读取失败: {error}");
        }
        request.push(byte[0]);
        if request.ends_with(b"\r\n\r\n") {
            return request;
        }
    }
    panic!("HTTP CONNECT fixture 请求头超限");
}

async fn await_task(task: tokio::task::JoinHandle<()>, label: &str) {
    match timeout(TEST_TIMEOUT, task).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("{label} 任务失败: {error}"),
        Err(_) => panic!("{label} 任务超时"),
    }
}
