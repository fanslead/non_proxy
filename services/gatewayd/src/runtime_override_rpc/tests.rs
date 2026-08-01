use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    control::v1::{OperationContext, SetRuntimeOverrideRequest},
    policy::v1::RuntimeOverrideMode,
};
use nonproxy_storage::{PolicyDatabase, ProviderAck};
use prost_types::Duration;
use tonic::Code;

use super::{set, status};
use crate::{
    Gateway, control_rpc_service::ControlRpcService, session_capability::SessionCapability,
};

#[tokio::test]
async fn set_requires_auth_duration_and_active_snapshot_version() {
    let service = service([7; 32]);
    let unauthorized = set(&service, request([8; 32], "pause-unauthorized", 300, 0, 1)).await;
    assert!(matches!(
        unauthorized,
        Err(value) if value.code() == Code::PermissionDenied
    ));

    let missing_version = set(&service, request([7; 32], "pause-no-version", 300, 0, 0)).await;
    assert!(matches!(
        missing_version,
        Err(value) if value.code() == Code::InvalidArgument
    ));

    let invalid_duration = set(
        &service,
        request([7; 32], "pause-subsecond", 0, 999_000_000, 1),
    )
    .await;
    assert!(matches!(
        invalid_duration,
        Err(value) if value.code() == Code::InvalidArgument
    ));
}

#[tokio::test]
async fn status_reports_pending_until_provider_acknowledgement() {
    let database = PolicyDatabase::open_in_memory(1)
        .unwrap_or_else(|error| panic!("运行态覆盖 RPC 数据库打开失败: {error}"));
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let initial = gateway
        .compile_and_stage()
        .await
        .unwrap_or_else(|error| panic!("运行态覆盖初始快照暂存失败: {error}"));
    activate_snapshot(&gateway, &initial).await;
    let service = ControlRpcService::new(gateway, SessionCapability::from_token([7; 32]));

    let response = set(&service, request([7; 32], "pause-five-minutes", 300, 0, 1))
        .await
        .unwrap_or_else(|error| panic!("运行态覆盖 RPC 暂存失败: {error}"));
    assert!(matches!(
        response.result.and_then(|value| value.snapshot),
        Some(snapshot) if snapshot.snapshot_version == 2
            && snapshot.state == nonproxy_proto::policy::v1::SnapshotState::PendingAck as i32
    ));

    let response = status(&service)
        .await
        .unwrap_or_else(|error| panic!("运行态覆盖 RPC 状态读取失败: {error}"));
    assert!(response.active_override.is_none());
    assert!(matches!(
        response.pending_override,
        Some(value) if value.mode == RuntimeOverrideMode::Paused as i32
    ));
    assert_eq!(response.active_snapshot_version, 1);
    assert_eq!(response.pending_snapshot_version, 2);
}

fn request(
    token: [u8; 32],
    operation_id: &str,
    seconds: i64,
    nanos: i32,
    expected_active_snapshot_version: u64,
) -> SetRuntimeOverrideRequest {
    SetRuntimeOverrideRequest {
        context: Some(OperationContext {
            operation_id: operation_id.to_owned(),
            session_capability_token: token.to_vec(),
        }),
        mode: RuntimeOverrideMode::Paused as i32,
        duration: Some(Duration { seconds, nanos }),
        outbound_id: String::new(),
        expected_active_snapshot_version,
    }
}

fn service(token: [u8; 32]) -> ControlRpcService {
    let database = PolicyDatabase::open_in_memory(1)
        .unwrap_or_else(|error| panic!("测试数据库打开失败: {error}"));
    ControlRpcService::new(
        Gateway::new(database, CompileCapabilities::full()),
        SessionCapability::from_token(token),
    )
}

async fn activate_snapshot(gateway: &Gateway, snapshot: &crate::PublishedSnapshot) {
    let required = crate::provider_requirements::required_provider_ids()
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    for (index, provider_id) in crate::provider_requirements::required_provider_ids()
        .iter()
        .enumerate()
    {
        let acknowledgement = ProviderAck::loaded(
            *provider_id,
            snapshot.artifact().snapshot_version(),
            *snapshot.artifact().content_hash(),
            1_000 + u64::try_from(index).unwrap_or(0),
        )
        .unwrap_or_else(|error| panic!("运行态覆盖测试 ACK 创建失败: {error}"));
        gateway
            .acknowledge_provider_snapshot(
                snapshot.artifact().snapshot_version(),
                acknowledgement,
                required.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("运行态覆盖测试 ACK 保存失败: {error}"));
    }
}
