use std::collections::HashMap;

use nonproxy_model::{ConnectionContext, DomainMatchKind, DomainName};

use crate::{CompiledRule, index::preferred_matching_rule};

#[derive(Clone, Debug, Default)]
pub(crate) struct DomainRuleIndex {
    exact: HashMap<String, Vec<CompiledRule>>,
    registrable: HashMap<String, Vec<CompiledRule>>,
    suffix: DomainTrieNode,
}

#[derive(Clone, Debug, Default)]
struct DomainTrieNode {
    rules: Vec<CompiledRule>,
    children: HashMap<String, Self>,
}

impl DomainRuleIndex {
    pub(crate) fn insert(&mut self, rule: CompiledRule) {
        let Some(matcher) = rule.matcher().domain() else {
            return;
        };
        let key = matcher.pattern().as_ascii().to_owned();
        match matcher.kind() {
            DomainMatchKind::Exact => {
                self.exact.entry(key).or_default().push(rule);
            }
            DomainMatchKind::RegistrableDomain => {
                self.registrable.entry(key).or_default().push(rule);
            }
            DomainMatchKind::Suffix => self.suffix.insert(&key, rule),
        }
    }

    pub(crate) fn best_match(&self, context: &ConnectionContext) -> Option<&CompiledRule> {
        let domain = context.destination().domain()?;
        let mut candidates = Vec::new();
        extend_for_key(&self.exact, domain.as_ascii(), &mut candidates);
        if let Some(registrable) = domain.registrable() {
            extend_for_key(&self.registrable, registrable, &mut candidates);
        }
        self.extend_suffix_candidates(domain, &mut candidates);
        preferred_matching_rule(candidates, context)
    }

    fn extend_suffix_candidates<'a>(
        &'a self,
        domain: &DomainName,
        candidates: &mut Vec<&'a CompiledRule>,
    ) {
        self.suffix.collect(domain.as_ascii(), candidates);
    }
}

impl DomainTrieNode {
    fn insert(&mut self, domain: &str, rule: CompiledRule) {
        let mut node = self;
        for label in domain.split('.').rev() {
            node = node.children.entry(label.to_owned()).or_default();
        }
        node.rules.push(rule);
    }

    fn collect<'a>(&'a self, domain: &str, candidates: &mut Vec<&'a CompiledRule>) {
        let mut node = self;
        for label in domain.split('.').rev() {
            let Some(child) = node.children.get(label) else {
                break;
            };
            node = child;
            candidates.extend(&node.rules);
        }
    }
}

fn extend_for_key<'a>(
    index: &'a HashMap<String, Vec<CompiledRule>>,
    key: &str,
    candidates: &mut Vec<&'a CompiledRule>,
) {
    if let Some(rules) = index.get(key) {
        candidates.extend(rules);
    }
}
