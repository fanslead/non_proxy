#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use hickory_proto::{
    op::{Message, MessageType, OpCode, Query},
    rr::{Name, RData, Record, RecordType, rdata::A},
};
use hyper_util::rt::TokioIo;
use nonproxy_proto::{
    common::v1::{AppIdentity, ComponentKind, ComponentVersion, Platform},
    control::v1::{
        ApplyPolicySnapshotRequest, GetSystemStatusRequest, OperationContext,
        control_service_client::ControlServiceClient,
    },
    provider::v1::{
        AcknowledgeSnapshotRequest, DnsRouteKind, DnsUpstreamEndpoint, GetCurrentSnapshotRequest,
        ProviderKind, ProviderRequestContext, RegisterProviderRequest, ReportHealthRequest,
        ResolveDnsRequest, provider_service_client::ProviderServiceClient,
    },
};
use tokio::{
    net::{UdpSocket, UnixStream},
    time::{Duration, sleep},
};
use tonic::{Code, transport::Endpoint};
use tower::service_fn;

#[tokio::test]
async fn two_authenticated_providers_activate_the_pending_snapshot() {
    let directory = secure_tempdir();
    let Ok(directory) = directory else {
        panic!("Provider RPC 临时目录创建失败: {directory:?}");
    };
    let state = directory.path().join("state");
    let process = GatewayProcess::start(&state);
    let Ok(mut process) = process else {
        panic!("Provider RPC gatewayd 启动失败: {process:?}");
    };
    wait_for_path(&state.join("gatewayd.sock")).await;
    wait_for_path(&state.join("session.capability")).await;
    wait_for_path(&state.join("provider.capability")).await;
    let control_capability = fs::read(state.join("session.capability"));
    let Ok(control_capability) = control_capability else {
        panic!("控制面 RPC 引导能力读取失败: {control_capability:?}");
    };
    let bootstrap = fs::read(state.join("provider.capability"));
    let Ok(bootstrap) = bootstrap else {
        panic!("Provider RPC 引导能力读取失败: {bootstrap:?}");
    };
    let channel = uds_channel(state.join("gatewayd.sock")).await;
    let Ok(channel) = channel else {
        panic!("Provider RPC UDS 连接失败: {channel:?}");
    };
    let mut control = ControlServiceClient::new(channel.clone());
    let staged = control
        .apply_policy_snapshot(ApplyPolicySnapshotRequest {
            context: Some(OperationContext {
                operation_id: "provider-rpc-stage".to_owned(),
                session_capability_token: control_capability,
            }),
        })
        .await;
    let Ok(staged) = staged else {
        panic!("Provider RPC 测试快照暂存失败: {staged:?}");
    };
    let Some(snapshot) = staged.into_inner().result.and_then(|value| value.snapshot) else {
        panic!("Provider RPC 测试缺少暂存快照");
    };
    assert_eq!(snapshot.snapshot_version, 1);

    let mut transparent = ProviderServiceClient::new(channel.clone());
    let transparent_session = register(
        &mut transparent,
        &bootstrap,
        "transparent-test-1",
        ProviderKind::TransparentProxy,
        ComponentKind::TransparentProxy,
        1,
    )
    .await;
    let current = transparent
        .get_current_snapshot(GetCurrentSnapshotRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                1,
            )),
            known_snapshot_version: 0,
        })
        .await;
    let Ok(current) = current else {
        panic!("Transparent Provider 快照获取失败: {current:?}");
    };
    let Some(candidate) = current.into_inner().snapshot else {
        panic!("Transparent Provider 未收到待确认快照");
    };
    let Some(metadata) = candidate.metadata else {
        panic!("Transparent Provider 快照缺少元数据");
    };
    let first_ack = transparent
        .acknowledge_snapshot(AcknowledgeSnapshotRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                2,
            )),
            snapshot_version: metadata.snapshot_version,
            content_hash: metadata.content_hash.clone(),
            accepted: true,
            error: None,
        })
        .await;
    let Ok(first_ack) = first_ack else {
        panic!("Transparent Provider 确认失败: {first_ack:?}");
    };
    assert_eq!(
        first_ack.into_inner().snapshot.map(|value| value.state),
        Some(nonproxy_proto::policy::v1::SnapshotState::PendingAck as i32)
    );

    let pending_retry = transparent
        .get_current_snapshot(GetCurrentSnapshotRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                3,
            )),
            known_snapshot_version: 1,
        })
        .await;
    let Ok(pending_retry) = pending_retry else {
        panic!("同版本待确认快照重投失败: {pending_retry:?}");
    };
    assert!(!pending_retry.into_inner().unchanged);

    let replay = transparent
        .get_current_snapshot(GetCurrentSnapshotRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                3,
            )),
            known_snapshot_version: 1,
        })
        .await;
    assert!(matches!(replay, Err(status) if status.code() == Code::PermissionDenied));

    let mut dns = ProviderServiceClient::new(channel);
    let dns_session = register(
        &mut dns,
        &bootstrap,
        "dns-test-1",
        ProviderKind::DnsProxy,
        ComponentKind::DnsProxy,
        2,
    )
    .await;
    let second_ack = dns
        .acknowledge_snapshot(AcknowledgeSnapshotRequest {
            context: Some(context("dns-test-1", &dns_session.session_token, 1)),
            snapshot_version: metadata.snapshot_version,
            content_hash: metadata.content_hash,
            accepted: true,
            error: None,
        })
        .await;
    let Ok(second_ack) = second_ack else {
        panic!("DNS Provider 确认失败: {second_ack:?}");
    };
    assert_eq!(
        second_ack.into_inner().snapshot.map(|value| value.state),
        Some(nonproxy_proto::policy::v1::SnapshotState::Active as i32)
    );
    let active_unchanged = transparent
        .get_current_snapshot(GetCurrentSnapshotRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                4,
            )),
            known_snapshot_version: 1,
        })
        .await;
    let Ok(active_unchanged) = active_unchanged else {
        panic!("同版本已激活快照检查失败: {active_unchanged:?}");
    };
    assert!(active_unchanged.into_inner().unchanged);

    let transparent_health = transparent
        .report_health(ReportHealthRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                5,
            )),
            state: nonproxy_proto::events::v1::RuntimeState::Ready as i32,
            active_snapshot_version: 1,
            active_flow_count: 0,
            queued_bytes: 0,
            error: None,
        })
        .await;
    assert!(transparent_health.is_ok());
    let dns_health = dns
        .report_health(ReportHealthRequest {
            context: Some(context("dns-test-1", &dns_session.session_token, 2)),
            state: nonproxy_proto::events::v1::RuntimeState::Ready as i32,
            active_snapshot_version: 1,
            active_flow_count: 0,
            queued_bytes: 0,
            error: None,
        })
        .await;
    assert!(dns_health.is_ok());
    let forbidden_dns = transparent
        .resolve_dns(ResolveDnsRequest {
            context: Some(context(
                "transparent-test-1",
                &transparent_session.session_token,
                6,
            )),
            ..Default::default()
        })
        .await;
    assert!(matches!(
        forbidden_dns,
        Err(status) if status.code() == Code::PermissionDenied
    ));

    let resolver = UdpSocket::bind("127.0.0.1:0").await;
    let Ok(resolver) = resolver else {
        panic!("DNS RPC resolver 夹具绑定失败: {resolver:?}");
    };
    let resolver_endpoint = resolver.local_addr();
    let Ok(resolver_endpoint) = resolver_endpoint else {
        panic!("DNS RPC resolver 地址读取失败: {resolver_endpoint:?}");
    };
    let resolver_task = tokio::spawn(async move {
        let mut query = vec![0_u8; u16::MAX as usize];
        let (received, peer) = resolver.recv_from(&mut query).await?;
        query.truncate(received);
        let response =
            dns_response_bytes(&query).map_err(|error| std::io::Error::other(error.to_string()))?;
        resolver.send_to(&response, peer).await?;
        Ok::<(), std::io::Error>(())
    });
    let query = dns_query_bytes(0xCAFE);
    let Ok(query) = query else {
        panic!("DNS RPC 查询构造失败: {query:?}");
    };
    let resolved = dns
        .resolve_dns(ResolveDnsRequest {
            context: Some(context("dns-test-1", &dns_session.session_token, 3)),
            query_id: "provider-rpc-dns".to_owned(),
            app: Some(AppIdentity {
                platform: Platform::Macos as i32,
                stable_id: "com.example.browser".to_owned(),
                ..Default::default()
            }),
            qname: "rpc.example".to_owned(),
            qtype: u32::from(u16::from(RecordType::A)),
            network_profile_id: "test-network".to_owned(),
            dns_message: query,
            requested_route: DnsRouteKind::Direct as i32,
            requested_outbound_id: String::new(),
            upstreams: vec![DnsUpstreamEndpoint {
                ip_address: resolver_endpoint.ip().to_string(),
                port: u32::from(resolver_endpoint.port()),
                scope_id: 0,
            }],
            snapshot_version: 1,
            direct_interface_index: 1,
        })
        .await;
    let Ok(resolved) = resolved else {
        panic!("DNS Provider RPC 解析失败: {resolved:?}");
    };
    let resolved = resolved.into_inner();
    assert!(resolved.error.is_none());
    assert_eq!(resolved.route, DnsRouteKind::Direct as i32);
    assert_eq!(resolved.resolver_endpoint, resolver_endpoint.to_string());
    let decoded = Message::from_vec(&resolved.dns_message);
    let Ok(decoded) = decoded else {
        panic!("DNS Provider RPC 响应解码失败: {decoded:?}");
    };
    assert_eq!(decoded.id, 0xCAFE);
    assert!(matches!(resolver_task.await, Ok(Ok(()))));

    let status = control.get_system_status(GetSystemStatusRequest {}).await;
    let Ok(status) = status else {
        panic!("Provider 确认后系统状态读取失败: {status:?}");
    };
    let status = status.into_inner();
    assert_eq!(status.active_snapshot_version, 1);
    assert_eq!(status.pending_snapshot_version, 0);
    assert!(status.data_plane_enabled);

    process.stop();
}

fn dns_query_bytes(id: u16) -> Result<Vec<u8>, hickory_proto::ProtoError> {
    let mut message = Message::new(id, MessageType::Query, OpCode::Query);
    message.add_query(Query::query(
        Name::from_ascii("rpc.example.")?,
        RecordType::A,
    ));
    message.to_vec()
}

fn dns_response_bytes(query: &[u8]) -> Result<Vec<u8>, hickory_proto::ProtoError> {
    let query = Message::from_vec(query)?;
    let question = query
        .queries
        .first()
        .cloned()
        .ok_or_else(|| hickory_proto::ProtoError::from("DNS RPC 查询缺少 question"))?;
    let mut response = Message::response(query.id, OpCode::Query);
    response.add_query(question.clone());
    response.add_answer(Record::from_rdata(
        question.name().clone(),
        60,
        RData::A(A::new(192, 0, 2, 7)),
    ));
    response.to_vec()
}

async fn register(
    client: &mut ProviderServiceClient<tonic::transport::Channel>,
    bootstrap: &[u8],
    instance_id: &str,
    kind: ProviderKind,
    component: ComponentKind,
    nonce_byte: u8,
) -> nonproxy_proto::provider::v1::RegisterProviderResponse {
    let response = client
        .register_provider(RegisterProviderRequest {
            provider_instance_id: instance_id.to_owned(),
            kind: kind as i32,
            version: Some(ComponentVersion {
                component: component as i32,
                semantic_version: "0.1.0".to_owned(),
                build_id: "provider-rpc-test".to_owned(),
                protocol_major: 1,
                protocol_minor: 0,
                minimum_protocol_minor: 0,
            }),
            capabilities: vec!["snapshot-v1".to_owned(), "heartbeat-v1".to_owned()],
            startup_nonce: vec![nonce_byte; 32],
            bootstrap_capability: bootstrap.to_vec(),
        })
        .await;
    let Ok(response) = response else {
        panic!("Provider 注册失败: {response:?}");
    };
    let response = response.into_inner();
    assert!(response.accepted);
    assert_eq!(response.current_snapshot_version, 1);
    assert_eq!(response.session_token.len(), 32);
    response
}

fn context(instance_id: &str, token: &[u8], sequence: u64) -> ProviderRequestContext {
    ProviderRequestContext {
        provider_instance_id: instance_id.to_owned(),
        session_token: token.to_vec(),
        request_sequence: sequence,
    }
}

async fn uds_channel(path: PathBuf) -> Result<tonic::transport::Channel, tonic::transport::Error> {
    Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_| {
            let path = path.clone();
            async move { UnixStream::connect(path).await.map(TokioIo::new) }
        }))
        .await
}

async fn wait_for_path(path: &Path) {
    for _attempt in 0..100 {
        if path.exists() {
            return;
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("Provider RPC 路径未在限定时间内创建: {}", path.display());
}

fn secure_tempdir() -> Result<tempfile::TempDir, std::io::Error> {
    let directory = tempfile::tempdir()?;
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
    Ok(directory)
}

#[derive(Debug)]
struct GatewayProcess {
    child: Child,
}

impl GatewayProcess {
    fn start(state_directory: &Path) -> Result<Self, std::io::Error> {
        let child = Command::new(env!("CARGO_BIN_EXE_nonproxy-gatewayd"))
            .env("NONPROXY_STATE_DIR", state_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;
        Ok(Self { child })
    }

    fn stop(&mut self) {
        let _kill_result = self.child.kill();
        let _wait_result = self.child.wait();
    }
}

impl Drop for GatewayProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
