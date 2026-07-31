use std::collections::{BTreeMap, HashSet};

use nonproxy_model::{DecisionSpec, NetworkProfileBinding, Policy, RuleId};
use nonproxy_policy::{CompiledPolicySnapshot, CompiledRule, SnapshotMetadata};

use crate::{
    CompileCapabilities, CompileError, PolicyConflict, canonical::content_hash,
    conflict::detect_conflicts,
};

pub const POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct CompileRequest {
    snapshot_version: u64,
    created_at_unix_ms: u64,
    default_decision: DecisionSpec,
    policies: Vec<Policy>,
    capabilities: CompileCapabilities,
    network_profiles: Option<Vec<NetworkProfileBinding>>,
}

impl CompileRequest {
    #[must_use]
    pub fn new(
        snapshot_version: u64,
        created_at_unix_ms: u64,
        default_decision: DecisionSpec,
        policies: Vec<Policy>,
        capabilities: CompileCapabilities,
    ) -> Self {
        Self {
            snapshot_version,
            created_at_unix_ms,
            default_decision,
            policies,
            capabilities,
            network_profiles: None,
        }
    }

    #[must_use]
    pub fn with_network_profiles(mut self, network_profiles: Vec<NetworkProfileBinding>) -> Self {
        self.network_profiles = Some(network_profiles);
        self
    }
}

pub struct PolicyCompiler;

impl PolicyCompiler {
    pub fn compile(request: CompileRequest) -> Result<CompiledPolicySnapshot, CompileError> {
        let mut conflicts = detect_conflicts(&request.policies);
        if request.snapshot_version == 0 {
            conflicts.push(PolicyConflict::global(
                "NP_POLICY_SNAPSHOT_VERSION_INVALID",
                "策略快照版本必须大于零",
            ));
        }
        request.capabilities.validate_target(&mut conflicts);
        request
            .capabilities
            .validate_default(&request.default_decision, &mut conflicts);
        for policy in request.policies.iter().filter(|policy| policy.enabled()) {
            request.capabilities.validate_policy(policy, &mut conflicts);
        }
        if let Some(network_profiles) = request.network_profiles.as_deref() {
            validate_network_profiles(&request.policies, network_profiles, &mut conflicts);
        }
        if !conflicts.is_empty() {
            return Err(CompileError::Validation { conflicts });
        }

        let mut enabled = request
            .policies
            .iter()
            .filter(|policy| policy.enabled())
            .collect::<Vec<_>>();
        enabled.sort_by(|left, right| left.id().cmp(right.id()));
        let hash = content_hash(
            POLICY_SCHEMA_VERSION,
            &request.default_decision,
            &enabled,
            request.capabilities.outbounds(),
            request.network_profiles.as_deref(),
        );
        let metadata = SnapshotMetadata::new(
            POLICY_SCHEMA_VERSION,
            request.snapshot_version,
            request.created_at_unix_ms,
            hash,
            enabled.len(),
        );
        let mut rules = Vec::with_capacity(enabled.len());
        for policy in enabled {
            let rule_id = RuleId::new(policy.id().as_str())?;
            rules.push(CompiledRule::from_policy(policy, rule_id));
        }

        let network_profiles = request
            .network_profiles
            .map_or_else(BTreeMap::new, |profiles| {
                profiles
                    .into_iter()
                    .map(|profile| (profile.id().clone(), profile.fingerprint().clone()))
                    .collect()
            });
        Ok(CompiledPolicySnapshot::from_compiled_rules(
            metadata,
            request.default_decision,
            request.capabilities.outbounds().clone(),
            network_profiles,
            rules,
        ))
    }
}

fn validate_network_profiles(
    policies: &[Policy],
    profiles: &[NetworkProfileBinding],
    conflicts: &mut Vec<PolicyConflict>,
) {
    let mut ids = HashSet::new();
    let mut fingerprints = HashSet::new();
    for profile in profiles {
        if !ids.insert(profile.id()) {
            conflicts.push(PolicyConflict::global(
                "NP_POLICY_NETWORK_PROFILE_DUPLICATE",
                "网络配置档标识重复",
            ));
        }
        if !fingerprints.insert(profile.fingerprint()) {
            conflicts.push(PolicyConflict::global(
                "NP_POLICY_NETWORK_FINGERPRINT_DUPLICATE",
                "网络配置档指纹重复",
            ));
        }
    }
    for policy in policies.iter().filter(|policy| policy.enabled()) {
        if let Some(network) = policy.matcher().network()
            && !ids.contains(network.profile_id())
        {
            conflicts.push(PolicyConflict::for_policy(
                "NP_POLICY_NETWORK_PROFILE_MISSING",
                "网络规则引用的配置档不存在",
                policy.id().clone(),
            ));
        }
    }
}
