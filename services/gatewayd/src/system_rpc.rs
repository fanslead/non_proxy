use nonproxy_proto::{
    common::v1::ComponentKind,
    control::v1::{ComponentStatus, GetCapabilitiesResponse, GetSystemStatusResponse},
    events::v1::RuntimeState,
};
use tonic::Status;

use crate::{
    clock::{timestamp_from_unix_ms, unix_time_ms},
    control_mapping,
    control_rpc_helpers::internal_status,
    control_rpc_service::ControlRpcService,
};

pub async fn status(service: &ControlRpcService) -> Result<GetSystemStatusResponse, Status> {
    let status = service.gateway.status().await.map_err(internal_status)?;
    let now = unix_time_ms().map_err(internal_status)?;
    let active_version = status
        .active
        .as_ref()
        .map_or(0, |record| record.artifact().snapshot_version());
    let pending_version = status
        .pending
        .as_ref()
        .map_or(0, |record| record.artifact().snapshot_version());
    let state = if status.data_plane_ready {
        RuntimeState::Ready
    } else {
        RuntimeState::Degraded
    };
    let component = ComponentStatus {
        component: ComponentKind::Gateway as i32,
        state: RuntimeState::Ready as i32,
        version: Some(control_mapping::gateway_component_version()),
        last_seen_at: Some(timestamp_from_unix_ms(now).map_err(internal_status)?),
        error: None,
    };
    let (default_route, default_outbound_id) =
        control_mapping::default_route(status.routing.route());
    Ok(GetSystemStatusResponse {
        state: state as i32,
        active_snapshot_version: active_version,
        data_plane_enabled: status.data_plane_ready,
        components: vec![component],
        latest_event_sequence: service
            .gateway
            .events()
            .latest_sequence()
            .map_err(internal_status)?,
        error: None,
        pending_snapshot_version: pending_version,
        default_route,
        default_outbound_id,
        routing_revision: status.routing.revision(),
    })
}

#[must_use]
pub fn capabilities(service: &ControlRpcService) -> GetCapabilitiesResponse {
    GetCapabilitiesResponse {
        capabilities: control_mapping::capability_names(service.gateway.capabilities()),
    }
}
