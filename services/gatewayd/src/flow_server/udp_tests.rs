use std::sync::Arc;

use nonproxy_flow_protocol::{
    DatagramPayload, FlowEndpoint, FlowFrame, FlowId, FrameType, OpenFlowRequest, read_frame,
    write_frame,
};
use nonproxy_model::OutboundId;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{OutboundKind, OutboundReference, PolicyDatabase};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket, UnixStream},
    time::{Duration, timeout},
};

use super::FlowConnectionHandler;
use crate::{
    Gateway,
    credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
    session_capability::SessionCapability,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(3);

#[tokio::test]
async fn authenticated_udp_flow_preserves_datagram_endpoint() {
    let listener = bind_tcp().await;
    let relay = bind_udp().await;
    let proxy_port = port_of_tcp(&listener);
    let proxy_task = tokio::spawn(socks5_udp_fixture(listener, relay));
    let gateway = gateway_with_socks_outbound(proxy_port).await;
    let token = [11_u8; 32];
    let handler = handler(gateway, token);
    let (server, mut client) = unix_pair();
    let handler_task = tokio::spawn(async move { handler.handle(server).await });
    let flow_id = flow_id();

    send_open_udp(&mut client, flow_id, token).await;
    assert_eq!(
        read_frame_with_timeout(&mut client).await.frame_type(),
        FrameType::WindowUpdate
    );
    let target = endpoint("dns.example", 53);
    let payload = match DatagramPayload::new(target.clone(), b"hello".to_vec())
        .and_then(|value| value.encode())
    {
        Ok(value) => value,
        Err(error) => panic!("测试 UDP payload 创建失败: {error}"),
    };
    send_frame(&mut client, FrameType::Datagram, flow_id, 1, &payload).await;

    let mut response = None;
    let mut acknowledged = false;
    while response.is_none() || !acknowledged {
        let frame = read_frame_with_timeout(&mut client).await;
        match frame.frame_type() {
            FrameType::Datagram => {
                response = Some(match DatagramPayload::decode(frame.payload()) {
                    Ok(value) => value,
                    Err(error) => panic!("测试 UDP 响应解码失败: {error}"),
                });
            }
            FrameType::WindowUpdate => acknowledged = true,
            other => panic!("收到非预期 UDP flow 帧: {other:?}"),
        }
    }
    let Some(response) = response else {
        panic!("缺少 UDP 回显响应");
    };
    assert_eq!(response.endpoint(), &target);
    assert_eq!(response.content(), b"hello");

    send_frame(&mut client, FrameType::Close, flow_id, 2, &[]).await;
    assert_eq!(
        read_until_terminal(&mut client).await.frame_type(),
        FrameType::Close
    );
    await_task(handler_task, "UDP flow handler").await;
    await_task(proxy_task, "SOCKS5 UDP fixture").await;
}

fn handler(gateway: Gateway, token: [u8; 32]) -> FlowConnectionHandler {
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    FlowConnectionHandler::new(gateway, SessionCapability::from_token(token), store)
}

async fn gateway_with_socks_outbound(port: u16) -> Gateway {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("测试数据库打开失败: {error}"),
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let outbound = match OutboundReference::new(
        outbound_id(),
        OutboundKind::Socks5,
        Some("127.0.0.1"),
        Some(port),
        None,
        1,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试 SOCKS5 出口创建失败: {error}"),
    };
    if let Err(error) = gateway.save_outbounds(vec![(outbound, None)]).await {
        panic!("测试 SOCKS5 出口保存失败: {error}");
    }
    gateway
}

async fn send_open_udp(stream: &mut UnixStream, flow_id: FlowId, token: [u8; 32]) {
    let request = match OpenFlowRequest::new(
        token,
        outbound_id(),
        endpoint("dns.example", 53),
        256 * 1024,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试 UDP OPEN 创建失败: {error}"),
    };
    let payload = match request.encode() {
        Ok(value) => value.to_vec(),
        Err(error) => panic!("测试 UDP OPEN 编码失败: {error}"),
    };
    send_frame(stream, FrameType::OpenUdp, flow_id, 0, &payload).await;
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
        Err(error) => panic!("测试 UDP flow 帧创建失败: {error}"),
    };
    if let Err(error) = write_frame(stream, &frame).await {
        panic!("测试 UDP flow 帧写入失败: {error}");
    }
}

async fn read_frame_with_timeout(stream: &mut UnixStream) -> FlowFrame {
    match timeout(TEST_TIMEOUT, read_frame(stream)).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => panic!("测试 UDP flow 帧读取失败: {error}"),
        Err(_) => panic!("测试 UDP flow 帧读取超时"),
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

async fn socks5_udp_fixture(listener: TcpListener, relay: UdpSocket) {
    let (mut control, _) = match listener.accept().await {
        Ok(value) => value,
        Err(error) => panic!("SOCKS5 UDP fixture 接收失败: {error}"),
    };
    let mut methods = [0_u8; 3];
    read_exact(&mut control, &mut methods).await;
    assert_eq!(methods, [5, 1, 0]);
    write_all(&mut control, &[5, 0]).await;
    let mut request = [0_u8; 10];
    read_exact(&mut control, &mut request).await;
    assert_eq!(&request[..4], &[5, 3, 0, 1]);
    let port = port_of_udp(&relay).to_be_bytes();
    write_all(&mut control, &[5, 0, 0, 1, 127, 0, 0, 1, port[0], port[1]]).await;

    let mut packet = [0_u8; 512];
    let (length, peer) = match timeout(TEST_TIMEOUT, relay.recv_from(&mut packet)).await {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => panic!("SOCKS5 UDP relay 接收失败: {error}"),
        Err(_) => panic!("SOCKS5 UDP relay 接收超时"),
    };
    assert_eq!(&packet[..4], &[0, 0, 0, 3]);
    assert_eq!(usize::from(packet[4]), "dns.example".len());
    if let Err(error) = relay.send_to(&packet[..length], peer).await {
        panic!("SOCKS5 UDP relay 回显失败: {error}");
    }
}

async fn read_exact(stream: &mut tokio::net::TcpStream, buffer: &mut [u8]) {
    if let Err(error) = stream.read_exact(buffer).await {
        panic!("SOCKS5 UDP fixture 读取失败: {error}");
    }
}

async fn write_all(stream: &mut tokio::net::TcpStream, buffer: &[u8]) {
    if let Err(error) = stream.write_all(buffer).await {
        panic!("SOCKS5 UDP fixture 写入失败: {error}");
    }
}

fn outbound_id() -> OutboundId {
    match OutboundId::new("proxy-udp") {
        Ok(value) => value,
        Err(error) => panic!("测试 UDP 出口 ID 创建失败: {error}"),
    }
}

fn endpoint(host: &str, port: u16) -> FlowEndpoint {
    match FlowEndpoint::new(host, port) {
        Ok(value) => value,
        Err(error) => panic!("测试 UDP endpoint 创建失败: {error}"),
    }
}

fn flow_id() -> FlowId {
    match FlowId::new([2_u8; 16]) {
        Ok(value) => value,
        Err(error) => panic!("测试 UDP flow ID 创建失败: {error}"),
    }
}

fn unix_pair() -> (UnixStream, UnixStream) {
    match UnixStream::pair() {
        Ok(value) => value,
        Err(error) => panic!("测试 UDP UnixStream pair 创建失败: {error}"),
    }
}

async fn bind_tcp() -> TcpListener {
    match TcpListener::bind(("127.0.0.1", 0)).await {
        Ok(value) => value,
        Err(error) => panic!("测试 SOCKS5 TCP fixture 绑定失败: {error}"),
    }
}

async fn bind_udp() -> UdpSocket {
    match UdpSocket::bind(("127.0.0.1", 0)).await {
        Ok(value) => value,
        Err(error) => panic!("测试 SOCKS5 UDP relay 绑定失败: {error}"),
    }
}

fn port_of_tcp(listener: &TcpListener) -> u16 {
    match listener.local_addr() {
        Ok(value) => value.port(),
        Err(error) => panic!("测试 SOCKS5 TCP 地址读取失败: {error}"),
    }
}

fn port_of_udp(socket: &UdpSocket) -> u16 {
    match socket.local_addr() {
        Ok(value) => value.port(),
        Err(error) => panic!("测试 SOCKS5 UDP 地址读取失败: {error}"),
    }
}

async fn await_task(task: tokio::task::JoinHandle<()>, label: &str) {
    match timeout(TEST_TIMEOUT, task).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("{label} 任务失败: {error}"),
        Err(_) => panic!("{label} 任务超时"),
    }
}
