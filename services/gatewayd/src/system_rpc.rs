use nonproxy_proto::{
    common::v1::ComponentKind,
    control::v1::{
        CapabilityName, ComponentStatus, GetCapabilitiesResponse, GetSystemStatusResponse,
    },
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
    let now = unix_time_ms().map_err(internal_status)?;
    let (status, system, provider_components) = service
        .gateway
        .runtime_status_at(now)
        .await
        .map_err(internal_status)?;
    let active_version = status
        .active
        .as_ref()
        .map_or(0, |record| record.artifact().snapshot_version());
    let pending_version = status
        .pending
        .as_ref()
        .map_or(0, |record| record.artifact().snapshot_version());
    let gateway_component = ComponentStatus {
        component: ComponentKind::Gateway as i32,
        state: RuntimeState::Ready as i32,
        version: Some(control_mapping::gateway_component_version()),
        last_seen_at: Some(timestamp_from_unix_ms(now).map_err(internal_status)?),
        error: None,
    };
    let mut components = Vec::with_capacity(provider_components.len() + 1);
    components.push(gateway_component);
    for provider in provider_components {
        components.push(ComponentStatus {
            component: provider.component() as i32,
            state: provider.state() as i32,
            version: None,
            last_seen_at: provider
                .last_seen_at_unix_ms()
                .map(timestamp_from_unix_ms)
                .transpose()
                .map_err(internal_status)?,
            error: provider.error().cloned(),
        });
    }
    let (default_route, default_outbound_id) =
        control_mapping::default_route(status.routing.route());
    Ok(GetSystemStatusResponse {
        state: system.state() as i32,
        active_snapshot_version: active_version,
        data_plane_enabled: system.data_plane_enabled(),
        components,
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
        dropped_decision_events: status.dropped_decision_events,
    })
}

#[must_use]
pub fn capabilities(service: &ControlRpcService) -> GetCapabilitiesResponse {
    let mut capabilities = control_mapping::capability_names(service.gateway.capabilities());
    if service.exit_probe_client.is_some() {
        capabilities.push(CapabilityName::ExitProbe as i32);
    }
    GetCapabilitiesResponse { capabilities }
}
