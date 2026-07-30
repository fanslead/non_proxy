#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use hyper_util::rt::TokioIo;
use nonproxy_proto::{
    common::v1::{ComponentKind, ComponentVersion},
    control::v1::{
        ApplyPolicySnapshotRequest, GetSystemStatusRequest, OperationContext,
        control_service_client::ControlServiceClient,
    },
    provider::v1::{
        AcknowledgeSnapshotRequest, GetCurrentSnapshotRequest, ProviderKind,
        ProviderRequestContext, RegisterProviderRequest, ReportHealthRequest,
        provider_service_client::ProviderServiceClient,
    },
};
use tokio::{
    net::UnixStream,
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
