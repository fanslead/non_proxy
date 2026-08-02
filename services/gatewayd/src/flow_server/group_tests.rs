use std::sync::Arc;

use nonproxy_flow_protocol::{
    FlowEndpoint, FlowFrame, FlowId, FlowProxyTarget, FlowReady, FrameType, OpenFlowRequest,
    read_frame, write_frame,
};
use nonproxy_model::{OutboundGroupId, OutboundId};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{
    OutboundGroup, OutboundGroupStrategy, OutboundKind, OutboundReference, PolicyDatabase,
    ProviderAck,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UnixStream},
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
async fn group_open_reports_and_uses_the_selected_snapshot_member() {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap_or_else(|error| panic!("组 flow 代理夹具绑定失败: {error}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("组 flow 代理夹具地址读取失败: {error}"))
        .port();
    let fixture = tokio::spawn(proxy_fixture(listener));
    let gateway = active_group_gateway(port).await;
    let now = crate::clock::unix_time_ms()
        .unwrap_or_else(|error| panic!("组 flow 健康时间读取失败: {error}"));
    for observed_at in [now, now] {
        gateway
            .report_outbound_health(
                outbound_id("backup"),
                1,
                RuntimeState::Ready,
                Some(5),
                observed_at,
            )
            .unwrap_or_else(|error| panic!("组 flow 健康状态写入失败: {error}"));
    }
    let token = [21_u8; 32];
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    let handler = FlowConnectionHandler::new(gateway, SessionCapability::from_token(token), store);
    let (server, mut client) =
        UnixStream::pair().unwrap_or_else(|error| panic!("组 flow UnixStream 创建失败: {error}"));
    let handler_task = tokio::spawn(async move { handler.handle(server).await });
    let flow_id =
        FlowId::new([3; 16]).unwrap_or_else(|error| panic!("组 flow ID 创建失败: {error}"));
    let open = OpenFlowRequest::new_with_target(
        token,
        FlowProxyTarget::Group {
            id: group_id(),
            snapshot_version: 1,
        },
        FlowEndpoint::new("example.test", 443)
            .unwrap_or_else(|error| panic!("组 flow 目标创建失败: {error}")),
        256 * 1024,
    )
    .unwrap_or_else(|error| panic!("组 flow OPEN 创建失败: {error}"));
    send(
        &mut client,
        FlowFrame::new(
            FrameType::OpenTcp,
            0,
            flow_id,
            0,
            open.encode()
                .unwrap_or_else(|error| panic!("组 flow OPEN 编码失败: {error}"))
                .to_vec(),
        )
        .unwrap_or_else(|error| panic!("组 flow OPEN 帧创建失败: {error}")),
    )
    .await;

    let ready = receive(&mut client).await;
    assert_eq!(ready.frame_type(), FrameType::Ready);
    assert!(matches!(
        FlowReady::decode(ready.payload()),
        Ok(value) if value.outbound_id().as_str() == "backup"
            && value.initial_window_bytes() == 256 * 1024
    ));
    send(
        &mut client,
        FlowFrame::new(FrameType::Data, 0, flow_id, 1, b"hello".to_vec())
            .unwrap_or_else(|error| panic!("组 flow DATA 帧创建失败: {error}")),
    )
    .await;
    let mut echoed = false;
    while !echoed {
        let frame = receive(&mut client).await;
        match frame.frame_type() {
            FrameType::Data => {
                assert_eq!(frame.payload(), b"hello");
                echoed = true;
            }
            FrameType::WindowUpdate => {}
            other => panic!("组 flow 收到非预期帧: {other:?}"),
        }
    }
    send(
        &mut client,
        FlowFrame::new(FrameType::Close, 0, flow_id, 2, Vec::new())
            .unwrap_or_else(|error| panic!("组 flow CLOSE 帧创建失败: {error}")),
    )
    .await;
    assert_eq!(receive_terminal(&mut client).await, FrameType::Close);
    assert!(matches!(handler_task.await, Ok(())));
    assert!(matches!(fixture.await, Ok(())));
}

async fn active_group_gateway(port: u16) -> Gateway {
    let database = PolicyDatabase::open_in_memory(1)
        .unwrap_or_else(|error| panic!("组 flow 数据库打开失败: {error}"));
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let outbounds = [("primary", 9_u16), ("backup", port)]
        .into_iter()
        .map(|(id, port)| {
            OutboundReference::new(
                outbound_id(id),
                OutboundKind::HttpConnect,
                Some("127.0.0.1"),
                Some(port),
                None,
                1,
            )
            .map(|value| (value, None))
            .unwrap_or_else(|error| panic!("组 flow 出口创建失败: {error}"))
        })
        .collect();
    gateway
        .save_outbounds(outbounds)
        .await
        .unwrap_or_else(|error| panic!("组 flow 出口保存失败: {error}"));
    gateway
        .save_outbound_group(
            OutboundGroup::new(
                group_id(),
                "自动切换",
                OutboundGroupStrategy::Failover,
                vec![outbound_id("primary"), outbound_id("backup")],
                1,
            )
            .unwrap_or_else(|error| panic!("组 flow 出口组创建失败: {error}")),
            None,
        )
        .await
        .unwrap_or_else(|error| panic!("组 flow 出口组保存失败: {error}"));
    let published = gateway
        .compile_and_stage()
        .await
        .unwrap_or_else(|error| panic!("组 flow 快照编译失败: {error}"));
    let ack = ProviderAck::loaded(
        "transparent-proxy",
        1,
        *published.artifact().content_hash(),
        2,
    )
    .unwrap_or_else(|error| panic!("组 flow 快照 ACK 创建失败: {error}"));
    gateway
        .acknowledge_provider_snapshot(1, ack, vec!["transparent-proxy".to_owned()])
        .await
        .unwrap_or_else(|error| panic!("组 flow 快照激活失败: {error}"));
    gateway
}

async fn proxy_fixture(listener: TcpListener) {
    let (mut stream, _) = listener
        .accept()
        .await
        .unwrap_or_else(|error| panic!("组 flow 代理夹具接收失败: {error}"));
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let byte = stream
            .read_u8()
            .await
            .unwrap_or_else(|error| panic!("组 flow 代理夹具读取失败: {error}"));
        header.push(byte);
    }
    assert!(String::from_utf8_lossy(&header).contains("CONNECT example.test:443"));
    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .unwrap_or_else(|error| panic!("组 flow 代理夹具响应失败: {error}"));
    let mut payload = [0_u8; 5];
    stream
        .read_exact(&mut payload)
        .await
        .unwrap_or_else(|error| panic!("组 flow 代理夹具数据读取失败: {error}"));
    stream
        .write_all(&payload)
        .await
        .unwrap_or_else(|error| panic!("组 flow 代理夹具回显失败: {error}"));
}

async fn send(stream: &mut UnixStream, frame: FlowFrame) {
    write_frame(stream, &frame)
        .await
        .unwrap_or_else(|error| panic!("组 flow 帧写入失败: {error}"));
}

async fn receive(stream: &mut UnixStream) -> FlowFrame {
    timeout(TEST_TIMEOUT, read_frame(stream))
        .await
        .unwrap_or_else(|_| panic!("组 flow 帧读取超时"))
        .unwrap_or_else(|error| panic!("组 flow 帧读取失败: {error}"))
}

async fn receive_terminal(stream: &mut UnixStream) -> FrameType {
    loop {
        let frame = receive(stream).await;
        if matches!(frame.frame_type(), FrameType::Close | FrameType::Error) {
            return frame.frame_type();
        }
    }
}

fn group_id() -> OutboundGroupId {
    OutboundGroupId::new("automatic")
        .unwrap_or_else(|error| panic!("组 flow 出口组 ID 无效: {error}"))
}

fn outbound_id(value: &str) -> OutboundId {
    OutboundId::new(value).unwrap_or_else(|error| panic!("组 flow 出口 ID 无效: {error}"))
}
