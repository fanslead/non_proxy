use nonproxy_model::{
    ConnectionContext, DecisionSpec, DomainMatchKind, Policy, PolicyId, PolicyMatch,
    PolicySourceKind, RuleId,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuleTier {
    BuiltIn = 1,
    Network = 2,
    Destination = 3,
    App = 4,
    AppDestination = 5,
    System = 6,
}

impl RuleTier {
    #[must_use]
    pub fn from_policy(policy: &Policy) -> Self {
        match policy.source_kind() {
            PolicySourceKind::System => Self::System,
            PolicySourceKind::AppDestination => Self::AppDestination,
            PolicySourceKind::App => Self::App,
            PolicySourceKind::Site | PolicySourceKind::Cidr => Self::Destination,
            PolicySourceKind::Network => Self::Network,
            PolicySourceKind::BuiltIn => Self::BuiltIn,
            PolicySourceKind::Adapter => {
                let matcher = policy.matcher();
                match (
                    matcher.app().is_some(),
                    matcher.domain().is_some() || matcher.cidr().is_some(),
                ) {
                    (true, true) => Self::AppDestination,
                    (true, false) => Self::App,
                    (false, true) => Self::Destination,
                    (false, false) => Self::Destination,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RuleSpecificity {
    app_signer: u8,
    destination_kind: u8,
    destination_depth: u16,
    transport: u8,
    port_narrowness: u32,
}

impl RuleSpecificity {
    #[must_use]
    pub fn from_matcher(matcher: &PolicyMatch) -> Self {
        let app_signer = u8::from(matcher.app().is_some_and(|app| app.signer_id().is_some()));
        let (destination_kind, destination_depth) = destination_specificity(matcher);
        let transport = u8::from(!matcher.transports().is_empty());
        let port_narrowness = if matcher.ports().is_empty() {
            0
        } else {
            let covered_ports = matcher
                .ports()
                .iter()
                .map(|range| u32::from(range.last() - range.first()) + 1)
                .sum::<u32>();
            u32::from(u16::MAX) + 1 - covered_ports
        };

        Self {
            app_signer,
            destination_kind,
            destination_depth,
            transport,
            port_narrowness,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledRule {
    policy_id: PolicyId,
    rule_id: RuleId,
    tier: RuleTier,
    matcher: PolicyMatch,
    decision: DecisionSpec,
    priority: i32,
    specificity: RuleSpecificity,
}

impl CompiledRule {
    #[must_use]
    pub fn from_policy(policy: &Policy, rule_id: RuleId) -> Self {
        Self {
            policy_id: policy.id().clone(),
            rule_id,
            tier: RuleTier::from_policy(policy),
            matcher: policy.matcher().clone(),
            decision: policy.decision().clone(),
            priority: policy.priority(),
            specificity: RuleSpecificity::from_matcher(policy.matcher()),
        }
    }

    #[must_use]
    pub const fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    #[must_use]
    pub const fn rule_id(&self) -> &RuleId {
        &self.rule_id
    }

    #[must_use]
    pub const fn tier(&self) -> RuleTier {
        self.tier
    }

    #[must_use]
    pub const fn matcher(&self) -> &PolicyMatch {
        &self.matcher
    }

    #[must_use]
    pub const fn decision(&self) -> &DecisionSpec {
        &self.decision
    }

    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    #[must_use]
    pub const fn specificity(&self) -> RuleSpecificity {
        self.specificity
    }

    #[must_use]
    pub fn matches(&self, context: &ConnectionContext) -> bool {
        let matcher = &self.matcher;
        if !matcher.matches_transport_and_port(
            context.destination().transport(),
            context.destination().port(),
        ) {
            return false;
        }
        if matcher.app().is_some_and(|app| !app.matches(context.app())) {
            return false;
        }
        if matcher.domain().is_some_and(|domain| {
            context
                .destination()
                .domain()
                .is_none_or(|destination| !domain.matches(destination))
        }) {
            return false;
        }
        if matcher.cidr().is_some_and(|cidr| {
            context
                .destination()
                .ip()
                .is_none_or(|address| !cidr.contains(address))
        }) {
            return false;
        }
        if matcher
            .network()
            .is_some_and(|network| context.network_profile_id() != Some(network.profile_id()))
        {
            return false;
        }
        true
    }

    #[must_use]
    pub fn is_preferred_to(&self, other: &Self) -> bool {
        self.priority > other.priority
            || (self.priority == other.priority
                && (self.specificity > other.specificity
                    || (self.specificity == other.specificity && self.policy_id < other.policy_id)))
    }
}

fn destination_specificity(matcher: &PolicyMatch) -> (u8, u16) {
    if let Some(domain) = matcher.domain() {
        let kind = match domain.kind() {
            DomainMatchKind::Suffix => 2,
            DomainMatchKind::RegistrableDomain => 3,
            DomainMatchKind::Exact => 4,
        };
        let depth = domain.pattern().as_ascii().split('.').count();
        let bounded_depth = u16::try_from(depth).unwrap_or(u16::MAX);
        return (kind, bounded_depth);
    }
    if let Some(cidr) = matcher.cidr() {
        return (1, u16::from(cidr.prefix_length()));
    }
    (0, 0)
}
