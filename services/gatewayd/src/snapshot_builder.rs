use nonproxy_model::{DecisionSpec, Policy};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{OutboundReference, SnapshotArtifact};

use crate::{
    GatewayError, PublishedSnapshot, outbound_capabilities, snapshot_payload, system_policies,
    system_policies::SystemPolicyConfig,
};

pub(crate) fn build_snapshot(
    capabilities: CompileCapabilities,
    policies: &[Policy],
    outbounds: &[OutboundReference],
    default_decision: DecisionSpec,
    snapshot_version: u64,
    created_at_unix_ms: u64,
    system_policy_config: &SystemPolicyConfig,
) -> Result<PublishedSnapshot, GatewayError> {
    let capabilities = outbound_capabilities::for_configured_outbounds(capabilities, outbounds);
    rebuild_snapshot(
        capabilities,
        policies,
        default_decision,
        snapshot_version,
        created_at_unix_ms,
        system_policy_config,
    )
}

pub(crate) fn rebuild_snapshot(
    capabilities: CompileCapabilities,
    policies: &[Policy],
    default_decision: DecisionSpec,
    snapshot_version: u64,
    created_at_unix_ms: u64,
    system_policy_config: &SystemPolicyConfig,
) -> Result<PublishedSnapshot, GatewayError> {
    let policies = system_policies::with_required(policies, system_policy_config)?;
    let compiled = PolicyCompiler::compile(CompileRequest::new(
        snapshot_version,
        created_at_unix_ms,
        default_decision.clone(),
        policies.clone(),
        capabilities.clone(),
    ))?;
    let payload = snapshot_payload::encode(&policies, &capabilities, &default_decision)?;
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
