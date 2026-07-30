use nonproxy_model::{DecisionSpec, Policy, RuleId};
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
        }
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

        Ok(CompiledPolicySnapshot::from_compiled_rules(
            metadata,
            request.default_decision,
            request.capabilities.outbounds().clone(),
            rules,
        ))
    }
}
