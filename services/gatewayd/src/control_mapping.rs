use nonproxy_model::{IpFamily, Transport};
use nonproxy_policy_compiler::{CompileCapabilities, CompileError};
use nonproxy_proto::{
    common::v1::{self as common_proto, ErrorDetail, PageResponse},
    control::v1::{
        CapabilityName, OutboundKind as ProtoOutboundKind, OutboundSummary,
        PolicyRuntimeState as ProtoPolicyRuntimeState, PolicyStatus,
    },
    events::v1::RuntimeState,
    policy::v1::{PolicyConflict as ProtoPolicyConflict, PolicySnapshotMetadata, SnapshotState},
};
use nonproxy_storage::{
    DefaultRoute, OutboundKind, OutboundReference, SnapshotArtifact, SnapshotStatus,
};

use crate::{
    GatewayError, RuntimePolicyRecord, RuntimePolicyState, clock::timestamp_from_unix_ms,
    outbound_health::OutboundHealthObservation, proto_policy::policy_to_proto,
};

pub fn error_detail(error: &GatewayError) -> ErrorDetail {
    ErrorDetail {
        code: error.code().to_owned(),
        message: error.to_string(),
        retryable: error.retryable(),
        metadata: Default::default(),
    }
}

#[must_use]
pub fn feature_unavailable(feature: &str) -> ErrorDetail {
    ErrorDetail {
        code: "NP_FEATURE_NOT_AVAILABLE".to_owned(),
        message: format!("{feature} 尚未在当前构建中启用。"),
        retryable: false,
        metadata: Default::default(),
    }
}

pub fn snapshot_metadata(
    artifact: &SnapshotArtifact,
    state: SnapshotState,
) -> Result<PolicySnapshotMetadata, GatewayError> {
    Ok(PolicySnapshotMetadata {
        schema_version: artifact.schema_version(),
        snapshot_version: artifact.snapshot_version(),
        content_hash: artifact.content_hash().to_vec(),
        state: state as i32,
        created_at: Some(timestamp_from_unix_ms(artifact.created_at_unix_ms())?),
        policy_count: u32::try_from(artifact.policy_count())
            .map_err(|_| GatewayError::InvalidRequest("策略数量超出协议范围"))?,
    })
}

#[must_use]
pub const fn snapshot_state(status: SnapshotStatus) -> SnapshotState {
    match status {
        SnapshotStatus::Pending => SnapshotState::PendingAck,
        SnapshotStatus::Active => SnapshotState::Active,
        SnapshotStatus::Superseded => SnapshotState::Superseded,
        SnapshotStatus::Rejected => SnapshotState::Rejected,
    }
}

#[must_use]
pub fn capability_names(capabilities: &CompileCapabilities) -> Vec<i32> {
    let mut names = Vec::new();
    if capabilities.supports_app_matching() {
        names.push(CapabilityName::AppMatch as i32);
    }
    if capabilities.supports_domain_matching() {
        names.push(CapabilityName::DomainMatch as i32);
    }
    if capabilities.supports_cidr_matching() {
        names.push(CapabilityName::CidrMatch as i32);
    }
    if capabilities.supports_transport(Transport::Tcp) {
        names.push(CapabilityName::Tcp as i32);
    }
    if capabilities.supports_transport(Transport::Udp) {
        names.push(CapabilityName::Udp as i32);
    }
    if capabilities.supports_family(IpFamily::Ipv4) {
        names.push(CapabilityName::Ipv4 as i32);
    }
    if capabilities.supports_family(IpFamily::Ipv6) {
        names.push(CapabilityName::Ipv6 as i32);
    }
    names
}

#[must_use]
pub fn outbound_summary(
    value: &OutboundReference,
    health: Option<&OutboundHealthObservation>,
    is_default: bool,
) -> OutboundSummary {
    let (kind, capabilities) = match value.kind() {
        OutboundKind::HttpConnect => (
            ProtoOutboundKind::HttpConnect,
            vec![
                CapabilityName::Tcp as i32,
                CapabilityName::Ipv4 as i32,
                CapabilityName::Ipv6 as i32,
            ],
        ),
        OutboundKind::Socks5 => (
            ProtoOutboundKind::Socks5,
            vec![
                CapabilityName::Tcp as i32,
                CapabilityName::Udp as i32,
                CapabilityName::Ipv4 as i32,
                CapabilityName::Ipv6 as i32,
            ],
        ),
        OutboundKind::Adapter => (ProtoOutboundKind::ExternalAdapter, Vec::new()),
    };
    OutboundSummary {
        id: value.id().as_str().to_owned(),
        display_name: value.id().as_str().to_owned(),
        kind: kind as i32,
        enabled: value.enabled(),
        health: health.map_or(RuntimeState::Unspecified, |value| value.state) as i32,
        capabilities,
        endpoint_host: value.endpoint_host().unwrap_or_default().to_owned(),
        endpoint_port: value.endpoint_port().map_or(0, u32::from),
        last_checked_at: health
            .and_then(|value| timestamp_from_unix_ms(value.observed_at_unix_ms).ok()),
        latency: health
            .and_then(|value| value.latency_ms)
            .and_then(duration_from_millis),
        is_default,
    }
}

#[must_use]
pub fn default_route(route: &DefaultRoute) -> (i32, String) {
    match route {
        DefaultRoute::Direct => (
            nonproxy_proto::control::v1::DefaultRouteKind::Direct as i32,
            String::new(),
        ),
        DefaultRoute::Proxy(outbound_id) => (
            nonproxy_proto::control::v1::DefaultRouteKind::Proxy as i32,
            outbound_id.as_str().to_owned(),
        ),
    }
}

fn duration_from_millis(value: u64) -> Option<prost_types::Duration> {
    let seconds = i64::try_from(value / 1_000).ok()?;
    let nanos = i32::try_from((value % 1_000) * 1_000_000).ok()?;
    Some(prost_types::Duration { seconds, nanos })
}

#[must_use]
pub fn policy_status(value: &RuntimePolicyRecord) -> PolicyStatus {
    let state = match value.state() {
        RuntimePolicyState::Draft => ProtoPolicyRuntimeState::Draft,
        RuntimePolicyState::Pending => ProtoPolicyRuntimeState::Pending,
        RuntimePolicyState::Active => ProtoPolicyRuntimeState::Active,
        RuntimePolicyState::PendingRemoval => ProtoPolicyRuntimeState::PendingRemoval,
    };
    PolicyStatus {
        policy: Some(policy_to_proto(value.policy())),
        state: state as i32,
        target_snapshot_version: value.target_snapshot_version().unwrap_or(0),
        effective_revision: value.effective_revision().unwrap_or(0),
        pending_revision: value.pending_revision().unwrap_or(0),
    }
}

#[must_use]
pub fn compile_conflicts(error: &CompileError) -> Vec<ProtoPolicyConflict> {
    error
        .conflicts()
        .iter()
        .map(|conflict| ProtoPolicyConflict {
            code: conflict.code().to_owned(),
            message: conflict.message().to_owned(),
            policy_ids: conflict
                .policy_ids()
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        })
        .collect()
}

pub fn page_bounds(
    page_size: u32,
    page_token: &str,
    total: usize,
) -> Result<(usize, usize, PageResponse), tonic::Status> {
    let size = match page_size {
        0 => 100_usize,
        1..=200 => usize::try_from(page_size)
            .map_err(|_| tonic::Status::invalid_argument("page_size 无效"))?,
        _ => return Err(tonic::Status::invalid_argument("page_size 最大为 200")),
    };
    let start = if page_token.is_empty() {
        0
    } else {
        page_token
            .parse::<usize>()
            .map_err(|_| tonic::Status::invalid_argument("page_token 无效"))?
    };
    if start > total {
        return Err(tonic::Status::invalid_argument("page_token 超出结果范围"));
    }
    let end = start.saturating_add(size).min(total);
    let next_page_token = if end < total {
        end.to_string()
    } else {
        String::new()
    };
    Ok((start, end, PageResponse { next_page_token }))
}

#[must_use]
pub fn gateway_component_version() -> common_proto::ComponentVersion {
    common_proto::ComponentVersion {
        component: common_proto::ComponentKind::Gateway as i32,
        semantic_version: env!("CARGO_PKG_VERSION").to_owned(),
        build_id: option_env!("NONPROXY_BUILD_ID")
            .unwrap_or("development")
            .to_owned(),
        protocol_major: 1,
        protocol_minor: 0,
        minimum_protocol_minor: 0,
    }
}
