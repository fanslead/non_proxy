use std::collections::HashMap;

use nonproxy_model::{ConnectionContext, Platform};

use crate::{CompiledRule, index::preferred_matching_rule};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AppKey {
    platform: Platform,
    stable_id: String,
}

impl AppKey {
    pub(crate) fn new(platform: Platform, stable_id: &str) -> Self {
        Self {
            platform,
            stable_id: stable_id.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AppRuleIndex {
    rules: HashMap<AppKey, Vec<CompiledRule>>,
}

impl AppRuleIndex {
    pub(crate) fn insert(&mut self, rule: CompiledRule) {
        let Some(matcher) = rule.matcher().app() else {
            return;
        };
        let key = AppKey::new(matcher.platform(), matcher.stable_id());
        self.rules.entry(key).or_default().push(rule);
    }

    pub(crate) fn best_match(&self, context: &ConnectionContext) -> Option<&CompiledRule> {
        let identity = context.app();
        let mut candidates = Vec::new();
        self.extend_candidates(&mut candidates, identity.platform(), identity.stable_id());
        if let Some(parent) = identity.parent_stable_id() {
            self.extend_candidates(&mut candidates, identity.platform(), parent);
        }
        if let Some(group) = identity.helper_group_id() {
            self.extend_candidates(&mut candidates, identity.platform(), group);
        }
        preferred_matching_rule(candidates, context)
    }

    fn extend_candidates<'a>(
        &'a self,
        candidates: &mut Vec<&'a CompiledRule>,
        platform: Platform,
        stable_id: &str,
    ) {
        if let Some(rules) = self.rules.get(&AppKey::new(platform, stable_id)) {
            candidates.extend(rules);
        }
    }
}
