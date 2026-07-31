use nonproxy_model::OutboundId;
use nonproxy_proto::{
    control::v1::{SetDefaultRouteRequest, SetDefaultRouteResponse, set_default_route_request},
    policy::v1::SnapshotState,
};
use nonproxy_storage::DefaultRoute;
use tonic::Status;

use crate::{
    GatewayError, control_mapping,
    control_rpc_helpers::{internal_status, publish_snapshot_event, request_status},
    control_rpc_service::ControlRpcService,
};

pub async fn set_default_route(
    service: &ControlRpcService,
    request: SetDefaultRouteRequest,
) -> Result<SetDefaultRouteResponse, Status> {
    service.session.validate(request.context.as_ref())?;
    if request.expected_routing_revision == 0 {
        return Err(Status::invalid_argument(
            "修改默认路由必须提供 expected_routing_revision",
        ));
    }
    let route = parse_route(request.route)?;
    let response = match service
        .gateway
        .set_default_route_and_stage(route, request.expected_routing_revision)
        .await
    {
        Ok(update) => {
            let metadata = control_mapping::snapshot_metadata(
                update.snapshot().artifact(),
                SnapshotState::PendingAck,
            )
            .map_err(internal_status)?;
            publish_snapshot_event(&service.gateway, metadata.clone())?;
            let _ = service.gateway.publish_runtime_events().await;
            SetDefaultRouteResponse {
                routing_revision: update.settings().revision(),
                snapshot: Some(metadata),
                error: None,
            }
        }
        Err(error) => SetDefaultRouteResponse {
            routing_revision: 0,
            snapshot: None,
            error: Some(control_mapping::error_detail(&error)),
        },
    };
    Ok(response)
}

fn parse_route(route: Option<set_default_route_request::Route>) -> Result<DefaultRoute, Status> {
    match route {
        Some(set_default_route_request::Route::Direct(true)) => Ok(DefaultRoute::Direct),
        Some(set_default_route_request::Route::Direct(false)) => {
            Err(Status::invalid_argument("direct 必须为 true"))
        }
        Some(set_default_route_request::Route::OutboundId(value)) => {
            let outbound_id = OutboundId::new(value)
                .map_err(|error| request_status(GatewayError::from(error)))?;
            Ok(DefaultRoute::Proxy(outbound_id))
        }
        None => Err(Status::invalid_argument("缺少默认路由目标")),
    }
}
