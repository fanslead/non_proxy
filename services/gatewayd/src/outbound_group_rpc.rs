use nonproxy_model::{OutboundGroupId, OutboundId};
use nonproxy_proto::{
    common::v1::PageRequest,
    control::v1::{
        DeleteOutboundGroupRequest, DeleteOutboundGroupResponse, ListOutboundGroupsRequest,
        ListOutboundGroupsResponse, OutboundGroupMutationResult, OutboundGroupStrategy,
        OutboundGroupSummary, UpsertOutboundGroupRequest, UpsertOutboundGroupResponse,
    },
};
use nonproxy_storage::{OutboundGroup, OutboundGroupStrategy as StoredOutboundGroupStrategy};
use tonic::Status;

use crate::{Gateway, GatewayError, control_mapping, control_rpc_helpers};

pub async fn list(
    gateway: &Gateway,
    request: ListOutboundGroupsRequest,
) -> Result<ListOutboundGroupsResponse, Status> {
    let groups = gateway
        .list_outbound_groups()
        .await
        .map_err(control_rpc_helpers::internal_status)?;
    let page = request.page.unwrap_or(PageRequest {
        page_size: 0,
        page_token: String::new(),
    });
    let (start, end, page) =
        control_mapping::page_bounds(page.page_size, &page.page_token, groups.len())?;
    Ok(ListOutboundGroupsResponse {
        groups: groups[start..end].iter().map(to_proto).collect(),
        page: Some(page),
    })
}

pub async fn upsert(
    gateway: &Gateway,
    request: UpsertOutboundGroupRequest,
) -> UpsertOutboundGroupResponse {
    let expected_revision = (request.expected_revision > 0).then_some(request.expected_revision);
    let result = match from_request(request) {
        Ok(group) => gateway
            .save_outbound_group(group, expected_revision)
            .await
            .map(|group| OutboundGroupMutationResult {
                group: Some(to_proto(&group)),
                error: None,
            })
            .unwrap_or_else(|error| mutation_error(&error)),
        Err(error) => mutation_error(&error),
    };
    UpsertOutboundGroupResponse {
        result: Some(result),
    }
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

fn to_proto(group: &OutboundGroup) -> OutboundGroupSummary {
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
    }
}

fn mutation_error(error: &GatewayError) -> OutboundGroupMutationResult {
    OutboundGroupMutationResult {
        group: None,
        error: Some(control_mapping::error_detail(error)),
    }
}
