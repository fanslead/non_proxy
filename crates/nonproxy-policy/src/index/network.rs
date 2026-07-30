use std::collections::HashMap;

use nonproxy_model::{ConnectionContext, NetworkProfileId};

use crate::{CompiledRule, index::preferred_matching_rule};

#[derive(Clone, Debug, Default)]
pub(crate) struct NetworkRuleIndex {
    rules: HashMap<NetworkProfileId, Vec<CompiledRule>>,
}

impl NetworkRuleIndex {
    pub(crate) fn insert(&mut self, rule: CompiledRule) {
        let Some(matcher) = rule.matcher().network() else {
            return;
        };
        self.rules
            .entry(matcher.profile_id().clone())
            .or_default()
            .push(rule);
    }

    pub(crate) fn best_match(&self, context: &ConnectionContext) -> Option<&CompiledRule> {
        let profile_id = context.network_profile_id()?;
        preferred_matching_rule(self.rules.get(profile_id)?, context)
    }
}
