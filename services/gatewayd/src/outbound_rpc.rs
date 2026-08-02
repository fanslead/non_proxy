use nonproxy_proto::{
    common::v1::PageRequest,
    control::v1::{ListOutboundsRequest, ListOutboundsResponse},
};
use nonproxy_storage::DefaultRoute;
use tonic::Status;

use crate::{Gateway, clock::unix_time_ms, control_mapping, control_rpc_helpers::internal_status};

pub async fn list(
    gateway: &Gateway,
    request: ListOutboundsRequest,
) -> Result<ListOutboundsResponse, Status> {
    let (outbounds, routing) = gateway
        .list_outbounds_with_routing()
        .await
        .map_err(internal_status)?;
    let page = request.page.unwrap_or(PageRequest {
        page_size: 0,
        page_token: String::new(),
    });
    let (start, end, page_response) =
        control_mapping::page_bounds(page.page_size, &page.page_token, outbounds.len())?;
    let now = unix_time_ms().map_err(internal_status)?;
    let health = outbounds[start..end]
        .iter()
        .map(|outbound| gateway.outbound_health(outbound, now))
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal_status)?;
    Ok(ListOutboundsResponse {
        outbounds: outbounds[start..end]
            .iter()
            .zip(health.iter())
            .map(|(outbound, health)| {
                let is_default = matches!(
                    routing.route(),
                    DefaultRoute::Proxy(outbound_id) if outbound_id == outbound.id()
                );
                control_mapping::outbound_summary(outbound, health.as_ref(), is_default)
            })
            .collect(),
        page: Some(page_response),
        routing_revision: routing.revision(),
        default_outbound_group_id: match routing.route() {
            DefaultRoute::Group(group_id) => group_id.as_str().to_owned(),
            DefaultRoute::Direct | DefaultRoute::Proxy(_) => String::new(),
        },
    })
}
