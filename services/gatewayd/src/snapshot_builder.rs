use nonproxy_model::{DecisionSpec, NetworkProfileBinding, Policy, RuntimeRoutingOverride};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{
    NetworkProfileReference, OutboundGroup, OutboundReference, SnapshotArtifact,
};

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

#[derive(Clone)]
pub(crate) struct SnapshotRoutingState {
    default_decision: DecisionSpec,
    runtime_override: Option<RuntimeRoutingOverride>,
}

#[derive(Clone, Copy)]
pub(crate) struct SnapshotCatalog<'catalog> {
    policies: &'catalog [Policy],
    outbounds: &'catalog [OutboundReference],
    outbound_groups: &'catalog [OutboundGroup],
    network_profiles: &'catalog [NetworkProfileReference],
}

impl<'catalog> SnapshotCatalog<'catalog> {
    #[must_use]
    pub(crate) const fn new(
        policies: &'catalog [Policy],
        outbounds: &'catalog [OutboundReference],
        outbound_groups: &'catalog [OutboundGroup],
        network_profiles: &'catalog [NetworkProfileReference],
    ) -> Self {
        Self {
            policies,
            outbounds,
            outbound_groups,
            network_profiles,
        }
    }

    #[must_use]
    pub(crate) const fn policies(self) -> &'catalog [Policy] {
        self.policies
    }

    #[must_use]
    pub(crate) const fn outbounds(self) -> &'catalog [OutboundReference] {
        self.outbounds
    }

    #[must_use]
    pub(crate) const fn outbound_groups(self) -> &'catalog [OutboundGroup] {
        self.outbound_groups
    }

    #[must_use]
    pub(crate) const fn network_profiles(self) -> &'catalog [NetworkProfileReference] {
        self.network_profiles
    }
}

impl SnapshotRoutingState {
    #[must_use]
    pub(crate) const fn new(
        default_decision: DecisionSpec,
        runtime_override: Option<RuntimeRoutingOverride>,
    ) -> Self {
        Self {
            default_decision,
            runtime_override,
        }
    }

    #[must_use]
    pub(crate) const fn default_decision(&self) -> &DecisionSpec {
        &self.default_decision
    }

    #[must_use]
    pub(crate) const fn runtime_override(&self) -> Option<&RuntimeRoutingOverride> {
        self.runtime_override.as_ref()
    }
}

pub(crate) fn build_snapshot(
    capabilities: CompileCapabilities,
    catalog: SnapshotCatalog<'_>,
    routing: SnapshotRoutingState,
    identity: SnapshotBuildIdentity,
    system_policy_config: &SystemPolicyConfig,
) -> Result<PublishedSnapshot, GatewayError> {
    let capabilities = outbound_capabilities::for_configured_outbounds(
        capabilities,
        catalog.outbounds(),
        catalog.outbound_groups(),
    )?;
    let network_profiles = catalog
        .network_profiles()
        .iter()
        .map(NetworkProfileReference::binding)
        .collect::<Vec<_>>();
    rebuild_snapshot(
        capabilities,
        catalog.policies(),
        &network_profiles,
        routing,
        identity,
        system_policy_config,
    )
}

pub(crate) fn rebuild_snapshot(
    capabilities: CompileCapabilities,
    policies: &[Policy],
    network_profiles: &[NetworkProfileBinding],
    routing: SnapshotRoutingState,
    identity: SnapshotBuildIdentity,
    system_policy_config: &SystemPolicyConfig,
) -> Result<PublishedSnapshot, GatewayError> {
    let SnapshotRoutingState {
        default_decision,
        runtime_override,
    } = routing;
    let policies = system_policies::with_required(policies, system_policy_config)?;
    let compiled = PolicyCompiler::compile(
        CompileRequest::new(
            identity.version,
            identity.created_at_unix_ms,
            default_decision.clone(),
            policies.clone(),
            capabilities.clone(),
        )
        .with_network_profiles(network_profiles.to_vec())
        .with_runtime_override(runtime_override.clone()),
    )?;
    let payload = snapshot_payload::encode(
        &policies,
        &capabilities,
        &default_decision,
        network_profiles,
        runtime_override.as_ref(),
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
