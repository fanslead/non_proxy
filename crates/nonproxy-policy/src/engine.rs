use nonproxy_model::{ConnectionContext, Decision, DecisionSpec, RuntimeOverrideMode};

use crate::{
    CompiledPolicySnapshot, CompiledRule,
    index::{prefer_optional_rules, preferred_matching_rule},
};

pub struct PolicyEngine;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyEvaluation {
    Bypass {
        snapshot_version: u64,
        reason_code: &'static str,
    },
    Decision(Decision),
}

impl PolicyEvaluation {
    #[must_use]
    pub const fn decision(&self) -> Option<&Decision> {
        match self {
            Self::Bypass { .. } => None,
            Self::Decision(decision) => Some(decision),
        }
    }
}

impl PolicyEngine {
    #[must_use]
    pub fn decide(snapshot: &CompiledPolicySnapshot, context: &ConnectionContext) -> Decision {
        if let Some(rule) = preferred_matching_rule(snapshot.system_rules(), context) {
            return matched(snapshot, rule, "NP_POLICY_SYSTEM_MATCH");
        }
        decide_after_system(snapshot, context)
    }

    #[must_use]
    pub fn evaluate_at(
        snapshot: &CompiledPolicySnapshot,
        context: &ConnectionContext,
        unix_time_ms: u64,
    ) -> PolicyEvaluation {
        if let Some(rule) = preferred_matching_rule(snapshot.system_rules(), context) {
            return PolicyEvaluation::Decision(matched(snapshot, rule, "NP_POLICY_SYSTEM_MATCH"));
        }
        if let Some(runtime_override) = snapshot
            .runtime_override()
            .filter(|value| value.is_active_at(unix_time_ms))
        {
            return match runtime_override.mode() {
                RuntimeOverrideMode::Paused => PolicyEvaluation::Bypass {
                    snapshot_version: snapshot.metadata().snapshot_version(),
                    reason_code: "NP_RUNTIME_OVERRIDE_PAUSED",
                },
                RuntimeOverrideMode::Direct | RuntimeOverrideMode::Proxy => {
                    match runtime_override.decision() {
                        Ok(Some(decision)) => PolicyEvaluation::Decision(Decision::defaulted(
                            decision,
                            snapshot.metadata().snapshot_version(),
                            match runtime_override.mode() {
                                RuntimeOverrideMode::Direct => "NP_RUNTIME_OVERRIDE_DIRECT",
                                RuntimeOverrideMode::Proxy => "NP_RUNTIME_OVERRIDE_PROXY",
                                RuntimeOverrideMode::Paused => "NP_RUNTIME_OVERRIDE_PAUSED",
                            },
                        )),
                        Ok(None) | Err(_) => PolicyEvaluation::Decision(Decision::defaulted(
                            DecisionSpec::blocked(),
                            snapshot.metadata().snapshot_version(),
                            "NP_RUNTIME_OVERRIDE_INVALID",
                        )),
                    }
                }
            };
        }
        PolicyEvaluation::Decision(decide_after_system(snapshot, context))
    }
}

fn decide_after_system(snapshot: &CompiledPolicySnapshot, context: &ConnectionContext) -> Decision {
    if let Some(rule) = snapshot.app_destination_rules().best_match(context) {
        return matched(snapshot, rule, "NP_POLICY_APP_DESTINATION_MATCH");
    }
    if let Some(rule) = snapshot.app_rules().best_match(context) {
        return matched(snapshot, rule, "NP_POLICY_APP_MATCH");
    }

    let domain = snapshot.domain_rules().best_match(context);
    let cidr = snapshot.cidr_rules().best_match(context);
    if let Some(rule) = prefer_optional_rules(domain, cidr) {
        return matched(snapshot, rule, "NP_POLICY_DESTINATION_MATCH");
    }

    if let Some(rule) = snapshot.network_rules().best_match(context) {
        return matched(snapshot, rule, "NP_POLICY_NETWORK_MATCH");
    }
    if let Some(rule) = preferred_matching_rule(snapshot.built_in_rules(), context) {
        return matched(snapshot, rule, "NP_POLICY_BUILTIN_MATCH");
    }
    Decision::defaulted(
        snapshot.default_decision().clone(),
        snapshot.metadata().snapshot_version(),
        "NP_POLICY_DEFAULT",
    )
}

fn matched(
    snapshot: &CompiledPolicySnapshot,
    rule: &CompiledRule,
    reason_code: &'static str,
) -> Decision {
    Decision::matched(
        rule.decision().clone(),
        rule.policy_id().clone(),
        rule.rule_id().clone(),
        snapshot.metadata().snapshot_version(),
        reason_code,
    )
}
