use std::collections::{BTreeMap, BTreeSet};

use nonproxy_model::{Policy, PolicyId};
use nonproxy_policy::RuleTier;

use crate::{PolicyConflict, canonical::matcher_bytes};

pub(crate) fn detect_conflicts(policies: &[Policy]) -> Vec<PolicyConflict> {
    let mut conflicts = Vec::new();
    detect_duplicate_ids(policies, &mut conflicts);
    detect_ambiguous_selectors(policies, &mut conflicts);
    conflicts
}

fn detect_duplicate_ids(policies: &[Policy], conflicts: &mut Vec<PolicyConflict>) {
    let mut counts = BTreeMap::<PolicyId, usize>::new();
    for policy in policies {
        *counts.entry(policy.id().clone()).or_default() += 1;
    }
    for (policy_id, count) in counts {
        if count > 1 {
            conflicts.push(PolicyConflict::for_policy(
                "NP_POLICY_DUPLICATE_ID",
                "策略标识重复",
                policy_id,
            ));
        }
    }
}

fn detect_ambiguous_selectors(policies: &[Policy], conflicts: &mut Vec<PolicyConflict>) {
    let mut selectors = BTreeMap::<(RuleTier, i32, Vec<u8>), BTreeSet<PolicyId>>::new();
    for policy in policies.iter().filter(|policy| policy.enabled()) {
        selectors
            .entry((
                RuleTier::from_policy(policy),
                policy.priority(),
                matcher_bytes(policy.matcher()),
            ))
            .or_default()
            .insert(policy.id().clone());
    }
    for (_, policy_ids) in selectors {
        if policy_ids.len() > 1 {
            conflicts.push(PolicyConflict::for_policies(
                "NP_POLICY_AMBIGUOUS_SELECTOR",
                "同一层级、优先级和选择器存在多条策略",
                policy_ids.into_iter().collect(),
            ));
        }
    }
}
