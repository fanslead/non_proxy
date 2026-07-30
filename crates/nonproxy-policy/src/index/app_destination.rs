use std::collections::HashMap;

use nonproxy_model::{ConnectionContext, Platform};

use super::app::AppKey;
use crate::{
    CompiledRule,
    index::{CidrRuleIndex, DomainRuleIndex, prefer_optional_rules},
};

#[derive(Clone, Debug, Default)]
struct DestinationRuleIndex {
    domain: DomainRuleIndex,
    cidr: CidrRuleIndex,
}

impl DestinationRuleIndex {
    fn insert(&mut self, rule: CompiledRule) {
        if rule.matcher().domain().is_some() {
            self.domain.insert(rule);
        } else {
            self.cidr.insert(rule);
        }
    }

    fn best_match(&self, context: &ConnectionContext) -> Option<&CompiledRule> {
        prefer_optional_rules(
            self.domain.best_match(context),
            self.cidr.best_match(context),
        )
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AppDestinationRuleIndex {
    rules: HashMap<AppKey, DestinationRuleIndex>,
}

impl AppDestinationRuleIndex {
    pub(crate) fn insert(&mut self, rule: CompiledRule) {
        let Some(matcher) = rule.matcher().app() else {
            return;
        };
        let key = AppKey::new(matcher.platform(), matcher.stable_id());
        self.rules.entry(key).or_default().insert(rule);
    }

    pub(crate) fn best_match(&self, context: &ConnectionContext) -> Option<&CompiledRule> {
        let identity = context.app();
        let mut best = self.best_for_key(identity.platform(), identity.stable_id(), context);
        if let Some(parent) = identity.parent_stable_id() {
            best = prefer_optional_rules(
                best,
                self.best_for_key(identity.platform(), parent, context),
            );
        }
        if let Some(group) = identity.helper_group_id() {
            best =
                prefer_optional_rules(best, self.best_for_key(identity.platform(), group, context));
        }
        best
    }

    fn best_for_key(
        &self,
        platform: Platform,
        stable_id: &str,
        context: &ConnectionContext,
    ) -> Option<&CompiledRule> {
        self.rules
            .get(&AppKey::new(platform, stable_id))
            .and_then(|index| index.best_match(context))
    }
}
