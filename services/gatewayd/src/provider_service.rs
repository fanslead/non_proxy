use std::collections::HashSet;

use nonproxy_proto::{
    common::v1::{ComponentKind, ComponentVersion, ErrorDetail},
    events::v1::RuntimeState,
    policy::v1::{CompiledPolicySnapshot, PolicySnapshotMetadata, SnapshotState},
    provider::v1::{
        self as provider_proto, AcknowledgeSnapshotRequest, AcknowledgeSnapshotResponse,
        CloseProxyFlowRequest, CloseProxyFlowResponse, GetCurrentSnapshotRequest,
        GetCurrentSnapshotResponse, OpenProxyFlowRequest, OpenProxyFlowResponse, ProviderKind,
        RegisterProviderRequest, RegisterProviderResponse, ReportDecisionBatchRequest,
        ReportDecisionBatchResponse, ReportHealthRequest, ReportHealthResponse, ResolveDnsRequest,
        ResolveDnsResponse, provider_service_server::ProviderService,
    },
};
use nonproxy_storage::{ProviderAck, SnapshotRecord, SnapshotStatus};
use prost_types::Duration;
use tonic::{Request, Response, Status};

use crate::{
    Gateway, ProviderSnapshot,
    clock::{timestamp_from_unix_ms, unix_time_ms},
    control_mapping,
    control_rpc_helpers::{internal_status, publish_snapshot_event},
    proto_policy::decision_to_proto,
    provider_requirements,
    provider_session::{ProviderSessionRegistry, validate_registration_input},
    session_capability::SessionCapability,
    snapshot_payload::SNAPSHOT_PAYLOAD_FORMAT,
};

const PROTOCOL_MAJOR: u32 = 1;
const PROTOCOL_MINOR: u32 = 0;
const MAX_CAPABILITIES: usize = 64;
const MAX_CAPABILITY_LENGTH: usize = 128;
const MAX_VERSION_FIELD_LENGTH: usize = 128;
const MAX_DECISIONS_PER_BATCH: usize = 1_000;
const REQUIRED_CAPABILITIES: &[&str] = &["snapshot-v1", "heartbeat-v1"];
#[derive(Clone)]
pub struct ProviderRpcService {
    gateway: Gateway,
    bootstrap: SessionCapability,
    sessions: ProviderSessionRegistry,
}

impl ProviderRpcService {
    #[must_use]
    pub fn new(gateway: Gateway, bootstrap: SessionCapability) -> Self {
        Self {
            gateway,
            bootstrap,
            sessions: ProviderSessionRegistry::new(),
        }
    }
}

#[tonic::async_trait]
impl ProviderService for ProviderRpcService {
    async fn register_provider(
        &self,
        request: Request<RegisterProviderRequest>,
    ) -> Result<Response<RegisterProviderResponse>, Status> {
        let request = request.into_inner();
        self.bootstrap
            .validate_token(&request.bootstrap_capability)?;
        let provider_id = validate_registration(&request)?;
        validate_registration_input(&request.provider_instance_id, &request.startup_nonce)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let generation = self
            .gateway
            .next_provider_generation(provider_id.to_owned())
            .await
            .map_err(internal_status)?;
        let now = unix_time_ms().map_err(internal_status)?;
        let session = self
            .sessions
            .register(
                request.provider_instance_id,
                provider_id.to_owned(),
                generation,
                &request.startup_nonce,
                now,
            )
            .map_err(internal_status)?;
        self.gateway
            .mark_provider_registered(provider_id, generation, now)
            .map_err(internal_status)?;
        let current_snapshot_version = self
            .gateway
            .provider_snapshot(0)
            .await
            .map_err(internal_status)?
            .as_ref()
            .map_or(0, |snapshot| {
                snapshot.record().artifact().snapshot_version()
            });
        let expires_at =
            timestamp_from_unix_ms(session.expires_at_unix_ms()).map_err(internal_status)?;
        Ok(Response::new(RegisterProviderResponse {
            accepted: true,
            negotiated_protocol_minor: PROTOCOL_MINOR,
            current_snapshot_version,
            session_token: session.token().to_vec(),
            error: None,
            session_expires_at: Some(expires_at),
            provider_generation: generation,
        }))
    }

    async fn get_current_snapshot(
        &self,
        request: Request<GetCurrentSnapshotRequest>,
    ) -> Result<Response<GetCurrentSnapshotResponse>, Status> {
        let request = request.into_inner();
        self.authenticate(request.context.as_ref())?;
        let snapshot = self
            .gateway
            .provider_snapshot(request.known_snapshot_version)
            .await
            .map_err(internal_status)?;
        Ok(Response::new(match snapshot {
            Some(snapshot) => GetCurrentSnapshotResponse {
                unchanged: false,
                snapshot: Some(snapshot_to_proto(&snapshot)?),
                error: None,
            },
            None => GetCurrentSnapshotResponse {
                unchanged: true,
                snapshot: None,
                error: None,
            },
        }))
    }

    async fn acknowledge_snapshot(
        &self,
        request: Request<AcknowledgeSnapshotRequest>,
    ) -> Result<Response<AcknowledgeSnapshotResponse>, Status> {
        let request = request.into_inner();
        let session = self.authenticate(request.context.as_ref())?;
        let content_hash: [u8; 32] = request
            .content_hash
            .try_into()
            .map_err(|_| Status::invalid_argument("content_hash 必须为 32 字节"))?;
        let now = unix_time_ms().map_err(internal_status)?;
        let acknowledgement = if request.accepted {
            ProviderAck::loaded(
                session.provider_id(),
                session.generation(),
                content_hash,
                now,
            )
        } else {
            let code = request
                .error
                .as_ref()
                .map(|error| error.code.as_str())
                .filter(|code| !code.is_empty())
                .ok_or_else(|| Status::invalid_argument("拒绝快照时必须提供错误码"))?;
            ProviderAck::rejected(
                session.provider_id(),
                session.generation(),
                content_hash,
                code,
                now,
            )
        }
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let required = provider_requirements::required_provider_ids()
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        let record = self
            .gateway
            .acknowledge_provider_snapshot(request.snapshot_version, acknowledgement, required)
            .await
            .map_err(internal_status)?;
        let metadata = metadata_for_record(&record)?;
        publish_snapshot_event(&self.gateway, metadata.clone())?;
        Ok(Response::new(AcknowledgeSnapshotResponse {
            snapshot: Some(metadata),
        }))
    }

    async fn report_decision_batch(
        &self,
        request: Request<ReportDecisionBatchRequest>,
    ) -> Result<Response<ReportDecisionBatchResponse>, Status> {
        let request = request.into_inner();
        self.authenticate(request.context.as_ref())?;
        if request.decisions.len() > MAX_DECISIONS_PER_BATCH {
            return Err(Status::resource_exhausted("单批决策记录最多 1000 条"));
        }
        Ok(Response::new(ReportDecisionBatchResponse {
            accepted_count: 0,
            error: Some(feature_unavailable("决策事件持久化")),
        }))
    }

    async fn report_health(
        &self,
        request: Request<ReportHealthRequest>,
    ) -> Result<Response<ReportHealthResponse>, Status> {
        let request = request.into_inner();
        let session = self.authenticate(request.context.as_ref())?;
        let state = RuntimeState::try_from(request.state)
            .map_err(|_| Status::invalid_argument("Provider 运行状态无效"))?;
        if state == RuntimeState::Unspecified {
            return Err(Status::invalid_argument("Provider 运行状态未指定"));
        }
        let now = unix_time_ms().map_err(internal_status)?;
        self.gateway
            .report_provider_health(
                session.provider_id(),
                session.generation(),
                state,
                request.active_snapshot_version,
                now,
            )
            .map_err(internal_status)?;
        Ok(Response::new(ReportHealthResponse {
            next_interval: Some(Duration {
                seconds: 5,
                nanos: 0,
            }),
        }))
    }

    async fn resolve_dns(
        &self,
        request: Request<ResolveDnsRequest>,
    ) -> Result<Response<ResolveDnsResponse>, Status> {
        self.authenticate(request.get_ref().context.as_ref())?;
        Ok(Response::new(ResolveDnsResponse {
            dns_message: Vec::new(),
            route: provider_proto::DnsRouteKind::Unspecified as i32,
            outbound_id: String::new(),
            valid_for: None,
            error: Some(feature_unavailable("DNS 解析")),
        }))
    }

    async fn open_proxy_flow(
        &self,
        request: Request<OpenProxyFlowRequest>,
    ) -> Result<Response<OpenProxyFlowResponse>, Status> {
        self.authenticate(request.get_ref().request_context.as_ref())?;
        Ok(Response::new(OpenProxyFlowResponse {
            accepted: false,
            frame_protocol_version: 1,
            initial_window_bytes: 0,
            error: Some(feature_unavailable("代理数据通道")),
            data_channel_token: Vec::new(),
        }))
    }

    async fn close_proxy_flow(
        &self,
        request: Request<CloseProxyFlowRequest>,
    ) -> Result<Response<CloseProxyFlowResponse>, Status> {
        self.authenticate(request.get_ref().context.as_ref())?;
        Ok(Response::new(CloseProxyFlowResponse { closed: false }))
    }
}

impl ProviderRpcService {
    fn authenticate(
        &self,
        context: Option<&provider_proto::ProviderRequestContext>,
    ) -> Result<crate::provider_session::ProviderSessionHandle, Status> {
        let now = unix_time_ms().map_err(internal_status)?;
        self.sessions.validate(context, now)
    }
}

fn validate_registration(request: &RegisterProviderRequest) -> Result<&'static str, Status> {
    let capabilities = request
        .capabilities
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if request.capabilities.len() > MAX_CAPABILITIES
        || capabilities.len() != request.capabilities.len()
        || request.capabilities.iter().any(|value| {
            value.is_empty()
                || value.len() > MAX_CAPABILITY_LENGTH
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
    {
        return Err(Status::invalid_argument("Provider capabilities 无效"));
    }
    if REQUIRED_CAPABILITIES
        .iter()
        .any(|required| !capabilities.contains(required))
    {
        return Err(Status::failed_precondition("Provider 未声明必需能力"));
    }
    let kind = ProviderKind::try_from(request.kind)
        .map_err(|_| Status::invalid_argument("Provider kind 无效"))?;
    let version = request
        .version
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("缺少 Provider 版本"))?;
    let (provider_id, component) = match kind {
        ProviderKind::TransparentProxy => ("transparent-proxy", ComponentKind::TransparentProxy),
        ProviderKind::DnsProxy => ("dns-proxy", ComponentKind::DnsProxy),
        ProviderKind::WindowsWfp => ("windows-wfp", ComponentKind::WindowsService),
        ProviderKind::WindowsDns => ("windows-dns", ComponentKind::WindowsService),
        ProviderKind::Unspecified => {
            return Err(Status::invalid_argument("Provider kind 未指定"));
        }
    };
    validate_version(version, component)?;
    Ok(provider_id)
}

fn validate_version(version: &ComponentVersion, component: ComponentKind) -> Result<(), Status> {
    if version.component != component as i32 {
        return Err(Status::failed_precondition(
            "Provider 组件类型与 kind 不一致",
        ));
    }
    if version.protocol_major != PROTOCOL_MAJOR
        || !(version.minimum_protocol_minor..=version.protocol_minor).contains(&PROTOCOL_MINOR)
    {
        return Err(Status::failed_precondition("Provider 协议版本不兼容"));
    }
    if !valid_version_field(&version.semantic_version) || !valid_version_field(&version.build_id) {
        return Err(Status::invalid_argument("Provider 构建版本不完整"));
    }
    Ok(())
}

fn valid_version_field(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VERSION_FIELD_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
}

fn snapshot_to_proto(snapshot: &ProviderSnapshot) -> Result<CompiledPolicySnapshot, Status> {
    let record = snapshot.record();
    Ok(CompiledPolicySnapshot {
        metadata: Some(metadata_for_record(record)?),
        payload_format: SNAPSHOT_PAYLOAD_FORMAT.to_owned(),
        payload: record.artifact().payload().to_vec(),
        default_decision: Some(decision_to_proto(snapshot.default_decision())),
    })
}

fn metadata_for_record(record: &SnapshotRecord) -> Result<PolicySnapshotMetadata, Status> {
    let state = match record.status() {
        SnapshotStatus::Pending => SnapshotState::PendingAck,
        SnapshotStatus::Active => SnapshotState::Active,
        SnapshotStatus::Rejected => SnapshotState::Rejected,
        SnapshotStatus::Superseded => SnapshotState::RolledBack,
    };
    control_mapping::snapshot_metadata(record.artifact(), state).map_err(internal_status)
}

fn feature_unavailable(feature: &str) -> ErrorDetail {
    ErrorDetail {
        code: "NP_FEATURE_NOT_AVAILABLE".to_owned(),
        message: format!("{feature}尚未在当前 Provider 版本启用。"),
        retryable: false,
        metadata: Default::default(),
    }
}

#[cfg(test)]
mod tests;
