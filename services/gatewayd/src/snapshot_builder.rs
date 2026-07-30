use nonproxy_model::{DecisionSpec, Policy};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{OutboundReference, SnapshotArtifact};

use crate::{GatewayError, PublishedSnapshot, outbound_capabilities, snapshot_payload};

pub(crate) fn build_snapshot(
    capabilities: CompileCapabilities,
    policies: &[Policy],
    outbounds: &[OutboundReference],
    snapshot_version: u64,
    created_at_unix_ms: u64,
) -> Result<PublishedSnapshot, GatewayError> {
    let capabilities = outbound_capabilities::for_configured_outbounds(capabilities, outbounds);
    let default_decision = DecisionSpec::direct();
    let compiled = PolicyCompiler::compile(CompileRequest::new(
        snapshot_version,
        created_at_unix_ms,
        default_decision.clone(),
        policies.to_vec(),
        capabilities.clone(),
    ))?;
    let payload = snapshot_payload::encode(policies, &capabilities, &default_decision)?;
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
