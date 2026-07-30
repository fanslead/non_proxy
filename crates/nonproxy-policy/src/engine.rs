use nonproxy_model::{ConnectionContext, Decision};

use crate::{
    CompiledPolicySnapshot, CompiledRule,
    index::{prefer_optional_rules, preferred_matching_rule},
};

pub struct PolicyEngine;

impl PolicyEngine {
    #[must_use]
    pub fn decide(snapshot: &CompiledPolicySnapshot, context: &ConnectionContext) -> Decision {
        if let Some(rule) = preferred_matching_rule(snapshot.system_rules(), context) {
            return matched(snapshot, rule, "NP_POLICY_SYSTEM_MATCH");
        }
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
