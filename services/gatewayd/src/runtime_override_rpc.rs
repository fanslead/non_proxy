use nonproxy_model::{OutboundId, RuntimeOverrideMode};
use nonproxy_policy_compiler::MAX_RUNTIME_OVERRIDE_DURATION_MS;
use nonproxy_proto::{
    control::v1::{
        ClearRuntimeOverrideRequest, ClearRuntimeOverrideResponse,
        GetRuntimeOverrideStatusResponse, PolicyMutationResult, SetRuntimeOverrideRequest,
        SetRuntimeOverrideResponse,
    },
    policy::v1::{RuntimeOverrideMode as ProtoRuntimeOverrideMode, SnapshotState},
};
use prost_types::Duration;
use tonic::Status;

use crate::{
    control_mapping,
    control_rpc_helpers::{mutation_error, publish_snapshot_event},
    control_rpc_service::ControlRpcService,
    snapshot_payload,
};

pub async fn status(
    service: &ControlRpcService,
) -> Result<GetRuntimeOverrideStatusResponse, Status> {
    let status = service
        .gateway
        .runtime_override_status()
        .await
        .map_err(crate::control_rpc_helpers::internal_status)?;
    Ok(GetRuntimeOverrideStatusResponse {
        active_override: status
            .active
            .as_ref()
            .map(snapshot_payload::runtime_override_to_proto)
            .transpose()
            .map_err(crate::control_rpc_helpers::internal_status)?,
        pending_override: status
            .pending
            .as_ref()
            .map(snapshot_payload::runtime_override_to_proto)
            .transpose()
            .map_err(crate::control_rpc_helpers::internal_status)?,
        active_snapshot_version: status.active_snapshot_version.unwrap_or(0),
        pending_snapshot_version: status.pending_snapshot_version.unwrap_or(0),
        pending_clears_override: status.pending_clears_override,
    })
}

pub async fn set(
    service: &ControlRpcService,
    request: SetRuntimeOverrideRequest,
) -> Result<SetRuntimeOverrideResponse, Status> {
    service.session.validate(request.context.as_ref())?;
    if request.expected_active_snapshot_version == 0 {
        return Err(Status::invalid_argument(
            "运行态覆盖必须提供当前活动快照版本",
        ));
    }
    let mode = parse_mode(request.mode)?;
    let outbound_id = parse_outbound(&request.outbound_id)?;
    let duration_ms = duration_ms(request.duration.as_ref())?;
    let result = match service
        .gateway
        .stage_runtime_override(
            mode,
            outbound_id,
            duration_ms,
            request.expected_active_snapshot_version,
        )
        .await
    {
        Ok(snapshot) => publish_result(service, snapshot).await?,
        Err(error) => mutation_error(&error),
    };
    Ok(SetRuntimeOverrideResponse {
        result: Some(result),
    })
}

pub async fn clear(
    service: &ControlRpcService,
    request: ClearRuntimeOverrideRequest,
) -> Result<ClearRuntimeOverrideResponse, Status> {
    service.session.validate(request.context.as_ref())?;
    if request.expected_active_snapshot_version == 0 {
        return Err(Status::invalid_argument(
            "取消运行态覆盖必须提供当前活动快照版本",
        ));
    }
    let result = match service
        .gateway
        .clear_runtime_override(request.expected_active_snapshot_version)
        .await
    {
        Ok(snapshot) => publish_result(service, snapshot).await?,
        Err(error) => mutation_error(&error),
    };
    Ok(ClearRuntimeOverrideResponse {
        result: Some(result),
    })
}

fn parse_mode(value: i32) -> Result<RuntimeOverrideMode, Status> {
    match ProtoRuntimeOverrideMode::try_from(value) {
        Ok(ProtoRuntimeOverrideMode::Paused) => Ok(RuntimeOverrideMode::Paused),
        Ok(ProtoRuntimeOverrideMode::Direct) => Ok(RuntimeOverrideMode::Direct),
        Ok(ProtoRuntimeOverrideMode::Proxy) => Ok(RuntimeOverrideMode::Proxy),
        Ok(ProtoRuntimeOverrideMode::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("运行态覆盖模式无效"))
        }
    }
}

fn parse_outbound(value: &str) -> Result<Option<OutboundId>, Status> {
    if value.is_empty() {
        return Ok(None);
    }
    OutboundId::new(value.to_owned())
        .map(Some)
        .map_err(|_| Status::invalid_argument("运行态覆盖出口标识无效"))
}

fn duration_ms(value: Option<&Duration>) -> Result<u64, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("运行态覆盖缺少时长"))?;
    if value.seconds < 0
        || value.nanos < 0
        || value.nanos >= 1_000_000_000
        || value.nanos % 1_000_000 != 0
    {
        return Err(Status::invalid_argument("运行态覆盖时长无效"));
    }
    let seconds =
        u64::try_from(value.seconds).map_err(|_| Status::invalid_argument("运行态覆盖时长无效"))?;
    let milliseconds = seconds
        .checked_mul(1_000)
        .and_then(|base| base.checked_add(u64::try_from(value.nanos / 1_000_000).ok()?))
        .filter(|value| *value >= 1_000 && *value <= MAX_RUNTIME_OVERRIDE_DURATION_MS)
        .ok_or_else(|| Status::invalid_argument("运行态覆盖时长必须在 1 秒到 1 小时之间"))?;
    Ok(milliseconds)
}

async fn publish_result(
    service: &ControlRpcService,
    snapshot: crate::PublishedSnapshot,
) -> Result<PolicyMutationResult, Status> {
    let metadata =
        control_mapping::snapshot_metadata(snapshot.artifact(), SnapshotState::PendingAck)
            .map_err(crate::control_rpc_helpers::internal_status)?;
    publish_snapshot_event(&service.gateway, metadata.clone())?;
    let _ = service.gateway.publish_runtime_events().await;
    Ok(PolicyMutationResult {
        policy: None,
        snapshot: Some(metadata),
        conflicts: Vec::new(),
        error: None,
    })
}

#[cfg(test)]
mod tests;
