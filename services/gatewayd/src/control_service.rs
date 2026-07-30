use std::pin::Pin;

use nonproxy_model::PolicyId;
use nonproxy_proto::{
    common::v1::{ComponentKind, PageRequest},
    control::v1::{
        self as control_proto, PolicyMutationResult, control_service_server::ControlService,
    },
    events::v1::RuntimeState,
    policy::v1::SnapshotState,
};
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status};

use crate::{
    Gateway, GatewayError,
    clock::{timestamp_from_unix_ms, unix_time_ms},
    control_mapping,
    control_rpc_helpers::{
        empty_mutation, event_meets_minimum, event_response, internal_status, minimum_severity,
        mutation_error, publish_snapshot_event, request_status,
    },
    proto_policy::{policy_from_proto, policy_to_proto},
    session_capability::SessionCapability,
};

const MAX_RPC_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct ControlRpcService {
    gateway: Gateway,
    session: SessionCapability,
}

impl ControlRpcService {
    #[must_use]
    pub const fn new(gateway: Gateway, session: SessionCapability) -> Self {
        Self { gateway, session }
    }

    #[must_use]
    pub const fn max_message_bytes() -> usize {
        MAX_RPC_MESSAGE_BYTES
    }
}

#[tonic::async_trait]
impl ControlService for ControlRpcService {
    async fn get_system_status(
        &self,
        _request: Request<control_proto::GetSystemStatusRequest>,
    ) -> Result<Response<control_proto::GetSystemStatusResponse>, Status> {
        let status = self.gateway.status().await.map_err(internal_status)?;
        let now = unix_time_ms().map_err(internal_status)?;
        let active_version = status
            .active
            .as_ref()
            .map_or(0, |record| record.artifact().snapshot_version());
        let state = if status.active.is_some() {
            RuntimeState::Ready
        } else {
            RuntimeState::Degraded
        };
        let component = control_proto::ComponentStatus {
            component: ComponentKind::Gateway as i32,
            state: RuntimeState::Ready as i32,
            version: Some(control_mapping::gateway_component_version()),
            last_seen_at: Some(timestamp_from_unix_ms(now).map_err(internal_status)?),
            error: None,
        };
        Ok(Response::new(control_proto::GetSystemStatusResponse {
            state: state as i32,
            active_snapshot_version: active_version,
            data_plane_enabled: status.active.is_some(),
            components: vec![component],
            latest_event_sequence: self
                .gateway
                .events()
                .latest_sequence()
                .map_err(internal_status)?,
            error: None,
        }))
    }

    async fn get_capabilities(
        &self,
        _request: Request<control_proto::GetCapabilitiesRequest>,
    ) -> Result<Response<control_proto::GetCapabilitiesResponse>, Status> {
        Ok(Response::new(control_proto::GetCapabilitiesResponse {
            capabilities: control_mapping::capability_names(self.gateway.capabilities()),
        }))
    }

    async fn list_policies(
        &self,
        request: Request<control_proto::ListPoliciesRequest>,
    ) -> Result<Response<control_proto::ListPoliciesResponse>, Status> {
        let request = request.into_inner();
        let mut policies = self
            .gateway
            .list_policies()
            .await
            .map_err(internal_status)?;
        if !request.include_disabled {
            policies.retain(nonproxy_model::Policy::enabled);
        }
        let page = request.page.unwrap_or(PageRequest {
            page_size: 0,
            page_token: String::new(),
        });
        let (start, end, page_response) =
            control_mapping::page_bounds(page.page_size, &page.page_token, policies.len())?;
        let active_version = self
            .gateway
            .status()
            .await
            .map_err(internal_status)?
            .active
            .map_or(0, |record| record.artifact().snapshot_version());
        Ok(Response::new(control_proto::ListPoliciesResponse {
            policies: policies[start..end].iter().map(policy_to_proto).collect(),
            page: Some(page_response),
            active_snapshot_version: active_version,
        }))
    }

    async fn upsert_policy(
        &self,
        request: Request<control_proto::UpsertPolicyRequest>,
    ) -> Result<Response<control_proto::UpsertPolicyResponse>, Status> {
        let request = request.into_inner();
        self.session.validate(request.context.as_ref())?;
        let policy = request
            .policy
            .ok_or_else(|| Status::invalid_argument("缺少 policy"))
            .and_then(|value| policy_from_proto(value).map_err(request_status))?;
        let expected = (request.expected_revision > 0).then_some(request.expected_revision);
        let result = match self.gateway.save_policy(policy, expected).await {
            Ok(saved) => PolicyMutationResult {
                policy: Some(policy_to_proto(&saved)),
                snapshot: None,
                conflicts: Vec::new(),
                error: None,
            },
            Err(error) => mutation_error(&error),
        };
        Ok(Response::new(control_proto::UpsertPolicyResponse {
            result: Some(result),
        }))
    }

    async fn delete_policy(
        &self,
        request: Request<control_proto::DeletePolicyRequest>,
    ) -> Result<Response<control_proto::DeletePolicyResponse>, Status> {
        let request = request.into_inner();
        self.session.validate(request.context.as_ref())?;
        if request.expected_revision == 0 {
            return Err(Status::invalid_argument(
                "删除策略必须提供 expected_revision",
            ));
        }
        let policy_id = PolicyId::new(request.policy_id)
            .map_err(|error| request_status(GatewayError::from(error)))?;
        let result = match self
            .gateway
            .delete_policy(policy_id, request.expected_revision)
            .await
        {
            Ok(()) => empty_mutation(),
            Err(error) => mutation_error(&error),
        };
        Ok(Response::new(control_proto::DeletePolicyResponse {
            result: Some(result),
        }))
    }

    async fn apply_policy_snapshot(
        &self,
        request: Request<control_proto::ApplyPolicySnapshotRequest>,
    ) -> Result<Response<control_proto::ApplyPolicySnapshotResponse>, Status> {
        let request = request.into_inner();
        self.session.validate(request.context.as_ref())?;
        let result = match self.gateway.compile_and_stage().await {
            Ok(snapshot) => {
                let metadata = control_mapping::snapshot_metadata(
                    snapshot.artifact(),
                    SnapshotState::PendingAck,
                )
                .map_err(internal_status)?;
                publish_snapshot_event(&self.gateway, metadata.clone())?;
                PolicyMutationResult {
                    policy: None,
                    snapshot: Some(metadata),
                    conflicts: Vec::new(),
                    error: None,
                }
            }
            Err(error) => mutation_error(&error),
        };
        Ok(Response::new(control_proto::ApplyPolicySnapshotResponse {
            result: Some(result),
        }))
    }

    async fn rollback_policy_snapshot(
        &self,
        request: Request<control_proto::RollbackPolicySnapshotRequest>,
    ) -> Result<Response<control_proto::RollbackPolicySnapshotResponse>, Status> {
        let request = request.into_inner();
        self.session.validate(request.context.as_ref())?;
        if request.target_snapshot_version == 0 {
            return Err(Status::invalid_argument("回滚目标快照版本无效"));
        }
        let result = match self
            .gateway
            .stage_rollback(request.target_snapshot_version)
            .await
        {
            Ok(snapshot) => {
                let metadata = control_mapping::snapshot_metadata(
                    snapshot.artifact(),
                    SnapshotState::PendingAck,
                )
                .map_err(internal_status)?;
                publish_snapshot_event(&self.gateway, metadata.clone())?;
                PolicyMutationResult {
                    policy: None,
                    snapshot: Some(metadata),
                    conflicts: Vec::new(),
                    error: None,
                }
            }
            Err(error) => mutation_error(&error),
        };
        Ok(Response::new(
            control_proto::RollbackPolicySnapshotResponse {
                result: Some(result),
            },
        ))
    }

    async fn list_outbounds(
        &self,
        request: Request<control_proto::ListOutboundsRequest>,
    ) -> Result<Response<control_proto::ListOutboundsResponse>, Status> {
        let request = request.into_inner();
        let outbounds = self
            .gateway
            .list_outbounds()
            .await
            .map_err(internal_status)?;
        let page = request.page.unwrap_or(PageRequest {
            page_size: 0,
            page_token: String::new(),
        });
        let (start, end, page_response) =
            control_mapping::page_bounds(page.page_size, &page.page_token, outbounds.len())?;
        Ok(Response::new(control_proto::ListOutboundsResponse {
            outbounds: outbounds[start..end]
                .iter()
                .map(control_mapping::outbound_summary)
                .collect(),
            page: Some(page_response),
        }))
    }

    async fn import_configuration(
        &self,
        request: Request<control_proto::ImportConfigurationRequest>,
    ) -> Result<Response<control_proto::ImportConfigurationResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(control_proto::ImportConfigurationResponse {
            import_id: String::new(),
            outbounds: Vec::new(),
            warnings: Vec::new(),
            error: Some(control_mapping::feature_unavailable("配置导入")),
        }))
    }

    async fn test_outbound(
        &self,
        request: Request<control_proto::TestOutboundRequest>,
    ) -> Result<Response<control_proto::TestOutboundResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(control_proto::TestOutboundResponse {
            healthy: false,
            latency: None,
            error: Some(control_mapping::feature_unavailable("出口探测")),
        }))
    }

    async fn start_learning_session(
        &self,
        request: Request<control_proto::StartLearningSessionRequest>,
    ) -> Result<Response<control_proto::StartLearningSessionResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(control_proto::StartLearningSessionResponse {
            session_id: String::new(),
            expires_at: None,
            error: Some(control_mapping::feature_unavailable("学习模式")),
        }))
    }

    async fn stop_learning_session(
        &self,
        request: Request<control_proto::StopLearningSessionRequest>,
    ) -> Result<Response<control_proto::StopLearningSessionResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(control_proto::StopLearningSessionResponse {
            candidate_count: 0,
            error: Some(control_mapping::feature_unavailable("学习模式")),
        }))
    }

    async fn export_diagnostics(
        &self,
        request: Request<control_proto::ExportDiagnosticsRequest>,
    ) -> Result<Response<control_proto::ExportDiagnosticsResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(control_proto::ExportDiagnosticsResponse {
            diagnostic_id: String::new(),
            local_path: String::new(),
            size_bytes: 0,
            sha256: Vec::new(),
            error: Some(control_mapping::feature_unavailable("诊断包导出")),
        }))
    }

    type SubscribeEventsStream =
        Pin<Box<dyn Stream<Item = Result<control_proto::SubscribeEventsResponse, Status>> + Send>>;

    async fn subscribe_events(
        &self,
        request: Request<control_proto::SubscribeEventsRequest>,
    ) -> Result<Response<Self::SubscribeEventsStream>, Status> {
        let request = request.into_inner();
        let minimum = minimum_severity(request.minimum_severity)?;
        let (backlog, receiver) = self
            .gateway
            .events()
            .subscribe(request.after_sequence)
            .map_err(internal_status)?;
        let backlog = tokio_stream::iter(
            backlog
                .into_iter()
                .filter(move |event| event_meets_minimum(event, minimum))
                .map(event_response),
        );
        let live = BroadcastStream::new(receiver).filter_map(move |event| match event {
            Ok(event) if event_meets_minimum(&event, minimum) => Some(event_response(event)),
            Ok(_) => None,
            Err(_) => Some(Err(Status::data_loss(
                "事件消费者落后，必须重新读取完整状态",
            ))),
        });
        let stream = backlog.chain(live);
        Ok(Response::new(Box::pin(stream)))
    }
}

#[cfg(test)]
mod tests;
