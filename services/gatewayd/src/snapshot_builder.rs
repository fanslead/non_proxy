use nonproxy_model::{DecisionSpec, NetworkProfileBinding, Policy};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{NetworkProfileReference, OutboundReference, SnapshotArtifact};

use crate::{
    GatewayError, PublishedSnapshot, outbound_capabilities, snapshot_payload, system_policies,
    system_policies::SystemPolicyConfig,
};

#[derive(Clone, Copy)]
pub(crate) struct SnapshotBuildIdentity {
    version: u64,
    created_at_unix_ms: u64,
}

impl SnapshotBuildIdentity {
    #[must_use]
    pub const fn new(version: u64, created_at_unix_ms: u64) -> Self {
        Self {
            version,
            created_at_unix_ms,
        }
    }
}

pub(crate) fn build_snapshot(
    capabilities: CompileCapabilities,
    policies: &[Policy],
    outbounds: &[OutboundReference],
    network_profiles: &[NetworkProfileReference],
    default_decision: DecisionSpec,
    identity: SnapshotBuildIdentity,
    system_policy_config: &SystemPolicyConfig,
) -> Result<PublishedSnapshot, GatewayError> {
    let capabilities = outbound_capabilities::for_configured_outbounds(capabilities, outbounds);
    let network_profiles = network_profiles
        .iter()
        .map(NetworkProfileReference::binding)
        .collect::<Vec<_>>();
    rebuild_snapshot(
        capabilities,
        policies,
        &network_profiles,
        default_decision,
        identity,
        system_policy_config,
    )
}

pub(crate) fn rebuild_snapshot(
    capabilities: CompileCapabilities,
    policies: &[Policy],
    network_profiles: &[NetworkProfileBinding],
    default_decision: DecisionSpec,
    identity: SnapshotBuildIdentity,
    system_policy_config: &SystemPolicyConfig,
) -> Result<PublishedSnapshot, GatewayError> {
    let policies = system_policies::with_required(policies, system_policy_config)?;
    let compiled = PolicyCompiler::compile(
        CompileRequest::new(
            identity.version,
            identity.created_at_unix_ms,
            default_decision.clone(),
            policies.clone(),
            capabilities.clone(),
        )
        .with_network_profiles(network_profiles.to_vec()),
    )?;
    let payload = snapshot_payload::encode(
        &policies,
        &capabilities,
        &default_decision,
        network_profiles,
    )?;
    let metadata = compiled.metadata();
    let artifact = SnapshotArtifact::new(
        metadata.snapshot_version(),
        metadata.schema_version(),
        metadata.created_at_unix_ms(),
        *metadata.content_hash(),
        metadata.policy_count(),
        payload,
    )?;
    Ok(PublishedSnapshot::new(artifact, default_decision))
}
