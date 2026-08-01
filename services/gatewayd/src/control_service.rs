use std::{pin::Pin, sync::Arc};

use nonproxy_model::PolicyId;
use nonproxy_proto::{
    common::v1::PageRequest,
    control::v1::{
        self as control_proto, PolicyMutationResult, control_service_server::ControlService,
    },
    policy::v1::SnapshotState,
};
use nonproxy_storage::DefaultRoute;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};
use tonic::{Request, Response, Status};

use crate::{
    GatewayError,
    clock::unix_time_ms,
    control_mapping,
    control_rpc_helpers::{
        empty_mutation, event_meets_minimum, event_response, internal_status, minimum_severity,
        mutation_error, publish_snapshot_event, request_status,
    },
    control_rpc_service::ControlRpcService,
    decision_rpc, diagnostics_export, exit_probe, exit_probe_rpc, learning_rpc,
    network_profile_rpc, outbound_import_service, outbound_probe,
    proto_policy::{policy_from_proto, policy_to_proto},
    routing_rpc, runtime_override_rpc, subscription_rpc, system_rpc,
};

#[tonic::async_trait]
impl ControlService for ControlRpcService {
    async fn get_system_status(
        &self,
        _request: Request<control_proto::GetSystemStatusRequest>,
    ) -> Result<Response<control_proto::GetSystemStatusResponse>, Status> {
        Ok(Response::new(system_rpc::status(self).await?))
    }

    async fn get_capabilities(
        &self,
        _request: Request<control_proto::GetCapabilitiesRequest>,
    ) -> Result<Response<control_proto::GetCapabilitiesResponse>, Status> {
        Ok(Response::new(system_rpc::capabilities(self)))
    }

    async fn list_policies(
        &self,
        request: Request<control_proto::ListPoliciesRequest>,
    ) -> Result<Response<control_proto::ListPoliciesResponse>, Status> {
        let request = request.into_inner();
        let catalog = self
            .gateway
            .runtime_policy_catalog()
            .await
            .map_err(internal_status)?;
        let generation = catalog.generation();
        let active_version = catalog.active_snapshot_version().unwrap_or(0);
        let pending_version = catalog.pending_snapshot_version().unwrap_or(0);
        let previous_effective_version = catalog.previous_effective_snapshot_version().unwrap_or(0);
        let mut policies = catalog.records().to_vec();
        if !request.include_disabled {
            policies.retain(|record| {
                record.policy().enabled() || record.effective_revision().is_some()
            });
        }
        let page = request.page.unwrap_or(PageRequest {
            page_size: 0,
            page_token: String::new(),
        });
        let (start, end, page_response) =
            control_mapping::page_bounds(page.page_size, &page.page_token, policies.len())?;
        Ok(Response::new(control_proto::ListPoliciesResponse {
            policies: policies[start..end]
                .iter()
                .map(|record| policy_to_proto(record.policy()))
                .collect(),
            page: Some(page_response),
            active_snapshot_version: active_version,
            pending_snapshot_version: pending_version,
            policy_statuses: policies[start..end]
                .iter()
                .map(control_mapping::policy_status)
                .collect(),
            policy_catalog_generation: generation,
            previous_effective_snapshot_version: previous_effective_version,
        }))
    }

    async fn get_active_policy_snapshot(
        &self,
        _request: Request<control_proto::GetActivePolicySnapshotRequest>,
    ) -> Result<Response<control_proto::GetActivePolicySnapshotResponse>, Status> {
        let snapshot = self
            .gateway
            .active_policy_snapshot()
            .await
            .map_err(internal_status)?;
        Ok(Response::new(match snapshot {
            Some(snapshot) => control_proto::GetActivePolicySnapshotResponse {
                snapshot_version: snapshot.snapshot_version,
                content_hash: snapshot.content_hash.to_vec(),
                policies: snapshot.policies.iter().map(policy_to_proto).collect(),
                error: None,
            },
            None => control_proto::GetActivePolicySnapshotResponse {
                snapshot_version: 0,
                content_hash: Vec::new(),
                policies: Vec::new(),
                error: None,
            },
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

    async fn list_network_profiles(
        &self,
        request: Request<control_proto::ListNetworkProfilesRequest>,
    ) -> Result<Response<control_proto::ListNetworkProfilesResponse>, Status> {
        Ok(Response::new(
            network_profile_rpc::list(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn upsert_network_profile(
        &self,
        request: Request<control_proto::UpsertNetworkProfileRequest>,
    ) -> Result<Response<control_proto::UpsertNetworkProfileResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            network_profile_rpc::upsert(&self.gateway, request.into_inner()).await,
        ))
    }

    async fn delete_network_profile(
        &self,
        request: Request<control_proto::DeleteNetworkProfileRequest>,
    ) -> Result<Response<control_proto::DeleteNetworkProfileResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            network_profile_rpc::delete(&self.gateway, request.into_inner()).await?,
        ))
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
                let _ = self.gateway.publish_runtime_events().await;
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
        if request.expected_active_snapshot_version == 0 {
            return Err(Status::invalid_argument("回滚必须提供当前活动快照版本"));
        }
        let result = match self
            .gateway
            .stage_rollback(
                request.target_snapshot_version,
                request.expected_active_snapshot_version,
            )
            .await
        {
            Ok(snapshot) => {
                let metadata = control_mapping::snapshot_metadata(
                    snapshot.artifact(),
                    SnapshotState::PendingAck,
                )
                .map_err(internal_status)?;
                publish_snapshot_event(&self.gateway, metadata.clone())?;
                let _ = self.gateway.publish_runtime_events().await;
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

    async fn get_runtime_override_status(
        &self,
        _request: Request<control_proto::GetRuntimeOverrideStatusRequest>,
    ) -> Result<Response<control_proto::GetRuntimeOverrideStatusResponse>, Status> {
        Ok(Response::new(runtime_override_rpc::status(self).await?))
    }

    async fn set_runtime_override(
        &self,
        request: Request<control_proto::SetRuntimeOverrideRequest>,
    ) -> Result<Response<control_proto::SetRuntimeOverrideResponse>, Status> {
        let response = runtime_override_rpc::set(self, request.into_inner()).await?;
        Ok(Response::new(response))
    }

    async fn clear_runtime_override(
        &self,
        request: Request<control_proto::ClearRuntimeOverrideRequest>,
    ) -> Result<Response<control_proto::ClearRuntimeOverrideResponse>, Status> {
        let response = runtime_override_rpc::clear(self, request.into_inner()).await?;
        Ok(Response::new(response))
    }

    async fn list_outbounds(
        &self,
        request: Request<control_proto::ListOutboundsRequest>,
    ) -> Result<Response<control_proto::ListOutboundsResponse>, Status> {
        let request = request.into_inner();
        let (outbounds, routing) = self
            .gateway
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
            .map(|outbound| self.gateway.outbound_health(outbound, now))
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal_status)?;
        Ok(Response::new(control_proto::ListOutboundsResponse {
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
        }))
    }

    async fn list_subscription_sources(
        &self,
        request: Request<control_proto::ListSubscriptionSourcesRequest>,
    ) -> Result<Response<control_proto::ListSubscriptionSourcesResponse>, Status> {
        Ok(Response::new(
            subscription_rpc::list(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn upsert_subscription_source(
        &self,
        request: Request<control_proto::UpsertSubscriptionSourceRequest>,
    ) -> Result<Response<control_proto::UpsertSubscriptionSourceResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            subscription_rpc::upsert(
                &self.subscription_service,
                &self.gateway,
                request.into_inner(),
            )
            .await,
        ))
    }

    async fn refresh_subscription_source(
        &self,
        request: Request<control_proto::RefreshSubscriptionSourceRequest>,
    ) -> Result<Response<control_proto::RefreshSubscriptionSourceResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            subscription_rpc::refresh(
                &self.subscription_service,
                &self.gateway,
                request.into_inner(),
            )
            .await,
        ))
    }

    async fn delete_subscription_source(
        &self,
        request: Request<control_proto::DeleteSubscriptionSourceRequest>,
    ) -> Result<Response<control_proto::DeleteSubscriptionSourceResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            subscription_rpc::delete(&self.subscription_service, request.into_inner()).await,
        ))
    }

    async fn list_connection_decisions(
        &self,
        request: Request<control_proto::ListConnectionDecisionsRequest>,
    ) -> Result<Response<control_proto::ListConnectionDecisionsResponse>, Status> {
        Ok(Response::new(
            decision_rpc::list(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn list_exit_probes(
        &self,
        request: Request<control_proto::ListExitProbesRequest>,
    ) -> Result<Response<control_proto::ListExitProbesResponse>, Status> {
        Ok(Response::new(
            exit_probe_rpc::list(
                &self.gateway,
                request.into_inner(),
                self.exit_probe_client.is_some(),
            )
            .await?,
        ))
    }

    async fn import_configuration(
        &self,
        request: Request<control_proto::ImportConfigurationRequest>,
    ) -> Result<Response<control_proto::ImportConfigurationResponse>, Status> {
        let request = request.into_inner();
        self.session.validate(request.context.as_ref())?;
        Ok(Response::new(
            outbound_import_service::import(
                &self.gateway,
                Arc::clone(&self.credential_store),
                request,
            )
            .await,
        ))
    }

    async fn test_outbound(
        &self,
        request: Request<control_proto::TestOutboundRequest>,
    ) -> Result<Response<control_proto::TestOutboundResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            outbound_probe::run(
                &self.gateway,
                Arc::clone(&self.credential_store),
                request.into_inner(),
            )
            .await,
        ))
    }

    async fn verify_exit(
        &self,
        request: Request<control_proto::VerifyExitRequest>,
    ) -> Result<Response<control_proto::VerifyExitResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            exit_probe::run(
                &self.gateway,
                Arc::clone(&self.credential_store),
                self.exit_probe_client.clone(),
                request.into_inner(),
            )
            .await,
        ))
    }

    async fn set_default_route(
        &self,
        request: Request<control_proto::SetDefaultRouteRequest>,
    ) -> Result<Response<control_proto::SetDefaultRouteResponse>, Status> {
        Ok(Response::new(
            routing_rpc::set_default_route(self, request.into_inner()).await?,
        ))
    }

    async fn start_learning_session(
        &self,
        request: Request<control_proto::StartLearningSessionRequest>,
    ) -> Result<Response<control_proto::StartLearningSessionResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            learning_rpc::start(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn record_learning_observation(
        &self,
        request: Request<control_proto::RecordLearningObservationRequest>,
    ) -> Result<Response<control_proto::RecordLearningObservationResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            learning_rpc::record(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn list_learning_candidates(
        &self,
        request: Request<control_proto::ListLearningCandidatesRequest>,
    ) -> Result<Response<control_proto::ListLearningCandidatesResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            learning_rpc::list(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn stop_learning_session(
        &self,
        request: Request<control_proto::StopLearningSessionRequest>,
    ) -> Result<Response<control_proto::StopLearningSessionResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            learning_rpc::stop(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn confirm_learning_candidates(
        &self,
        request: Request<control_proto::ConfirmLearningCandidatesRequest>,
    ) -> Result<Response<control_proto::ConfirmLearningCandidatesResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        Ok(Response::new(
            learning_rpc::confirm(&self.gateway, request.into_inner()).await?,
        ))
    }

    async fn export_diagnostics(
        &self,
        request: Request<control_proto::ExportDiagnosticsRequest>,
    ) -> Result<Response<control_proto::ExportDiagnosticsResponse>, Status> {
        self.session.validate(request.get_ref().context.as_ref())?;
        let Some(directory) = self.diagnostics_directory.as_deref() else {
            return Ok(Response::new(diagnostics_export::unavailable_response()));
        };
        match diagnostics_export::export(&self.gateway, directory, request.into_inner()).await {
            Ok(exported) => Ok(Response::new(exported.into_response()?)),
            Err(error) if error.is_invalid_request() => {
                Err(Status::invalid_argument(error.user_message()))
            }
            Err(error) => Ok(Response::new(error.into_response())),
        }
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
