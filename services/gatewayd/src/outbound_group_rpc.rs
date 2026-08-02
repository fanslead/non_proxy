use nonproxy_model::{OutboundGroupId, OutboundId};
use nonproxy_proto::{
    common::v1::PageRequest,
    control::v1::{
        DeleteOutboundGroupRequest, DeleteOutboundGroupResponse, ListOutboundGroupsRequest,
        ListOutboundGroupsResponse, OutboundGroupMutationResult, OutboundGroupStrategy,
        OutboundGroupSummary, UpsertOutboundGroupRequest, UpsertOutboundGroupResponse,
    },
    policy::v1::SnapshotState,
};
use nonproxy_storage::{
    DefaultRoute, OutboundGroup, OutboundGroupStrategy as StoredOutboundGroupStrategy,
};
use tonic::Status;

use crate::{
    Gateway, GatewayError, control_mapping,
    control_rpc_helpers::{self, publish_snapshot_event},
    control_rpc_service::ControlRpcService,
};

pub async fn list(
    gateway: &Gateway,
    request: ListOutboundGroupsRequest,
) -> Result<ListOutboundGroupsResponse, Status> {
    let (groups, routing) = gateway
        .list_outbound_groups_with_routing()
        .await
        .map_err(control_rpc_helpers::internal_status)?;
    let page = request.page.unwrap_or(PageRequest {
        page_size: 0,
        page_token: String::new(),
    });
    let (start, end, page) =
        control_mapping::page_bounds(page.page_size, &page.page_token, groups.len())?;
    Ok(ListOutboundGroupsResponse {
        groups: groups[start..end]
            .iter()
            .map(|group| {
                let is_default = matches!(
                    routing.route(),
                    DefaultRoute::Group(group_id) if group_id == group.id()
                );
                to_proto(group, is_default)
            })
            .collect(),
        page: Some(page),
        routing_revision: routing.revision(),
    })
}

pub async fn upsert(
    service: &ControlRpcService,
    request: UpsertOutboundGroupRequest,
) -> Result<UpsertOutboundGroupResponse, Status> {
    let expected_revision = (request.expected_revision > 0).then_some(request.expected_revision);
    let result = match from_request(request) {
        Ok(group) => match service
            .gateway
            .save_outbound_group(group, expected_revision)
            .await
        {
            Ok(saved) => {
                let is_default = matches!(
                    saved.routing().route(),
                    DefaultRoute::Group(group_id) if group_id == saved.group().id()
                );
                let snapshot = saved
                    .snapshot()
                    .map(|snapshot| {
                        control_mapping::snapshot_metadata(
                            snapshot.artifact(),
                            SnapshotState::PendingAck,
                        )
                    })
                    .transpose()
                    .map_err(control_rpc_helpers::internal_status)?;
                if let Some(metadata) = snapshot.as_ref() {
                    publish_snapshot_event(&service.gateway, metadata.clone())?;
                    let _ = service.gateway.publish_runtime_events().await;
                }
                OutboundGroupMutationResult {
                    group: Some(to_proto(saved.group(), is_default)),
                    error: None,
                    snapshot,
                    routing_revision: saved.routing().revision(),
                }
            }
            Err(error) => mutation_error(&error),
        },
        Err(error) => mutation_error(&error),
    };
    Ok(UpsertOutboundGroupResponse {
        result: Some(result),
    })
}

pub async fn delete(
    gateway: &Gateway,
    request: DeleteOutboundGroupRequest,
) -> Result<DeleteOutboundGroupResponse, Status> {
    if request.expected_revision == 0 {
        return Err(Status::invalid_argument(
            "删除出口组必须提供 expected_revision",
        ));
    }
    let group_id = OutboundGroupId::new(request.group_id)
        .map_err(GatewayError::from)
        .map_err(control_rpc_helpers::request_status)?;
    let response_id = group_id.as_str().to_owned();
    let error = gateway
        .delete_outbound_group(group_id, request.expected_revision)
        .await
        .err()
        .map(|error| control_mapping::error_detail(&error));
    Ok(DeleteOutboundGroupResponse {
        group_id: response_id,
        error,
    })
}

fn from_request(request: UpsertOutboundGroupRequest) -> Result<OutboundGroup, GatewayError> {
    let id = OutboundGroupId::new(request.group_id)?;
    let strategy = OutboundGroupStrategy::try_from(request.strategy)
        .map_err(|_| GatewayError::InvalidContract("出口组策略无效"))?;
    let strategy = match strategy {
        OutboundGroupStrategy::Failover => StoredOutboundGroupStrategy::Failover,
        OutboundGroupStrategy::Unspecified => {
            return Err(GatewayError::InvalidContract("缺少出口组策略"));
        }
    };
    let members = request
        .outbound_ids
        .into_iter()
        .map(OutboundId::new)
        .collect::<Result<Vec<_>, _>>()?;
    let revision = match request.expected_revision {
        0 => 1,
        value => value
            .checked_add(1)
            .ok_or(GatewayError::InvalidContract("出口组修订号已耗尽"))?,
    };
    OutboundGroup::new(id, request.display_name, strategy, members, revision)
        .map_err(GatewayError::from)
}

fn to_proto(group: &OutboundGroup, is_default: bool) -> OutboundGroupSummary {
    OutboundGroupSummary {
        id: group.id().as_str().to_owned(),
        display_name: group.display_name().to_owned(),
        strategy: OutboundGroupStrategy::Failover as i32,
        outbound_ids: group
            .members()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        revision: group.revision(),
        is_default,
    }
}

fn mutation_error(error: &GatewayError) -> OutboundGroupMutationResult {
    OutboundGroupMutationResult {
        group: None,
        error: Some(control_mapping::error_detail(error)),
        snapshot: None,
        routing_revision: 0,
    }
}
