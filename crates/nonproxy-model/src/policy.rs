use crate::{
    AppMatcher, Cidr, DecisionSpec, DomainMatcher, ModelError, NetworkProfileId, PolicyId,
    PortRange, Transport,
};

const MAX_POLICY_DISPLAY_NAME_LENGTH: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicySourceKind {
    System,
    AppDestination,
    App,
    Site,
    Network,
    BuiltIn,
    Cidr,
    Adapter,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyOrigin {
    System,
    User,
    SignedBuiltIn,
    Subscription,
    Adapter,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NetworkMatcher {
    profile_id: NetworkProfileId,
}

impl NetworkMatcher {
    #[must_use]
    pub const fn new(profile_id: NetworkProfileId) -> Self {
        Self { profile_id }
    }

    #[must_use]
    pub const fn profile_id(&self) -> &NetworkProfileId {
        &self.profile_id
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PolicyMatch {
    app: Option<AppMatcher>,
    domain: Option<DomainMatcher>,
    cidr: Option<Cidr>,
    network: Option<NetworkMatcher>,
    transports: Vec<Transport>,
    ports: Vec<PortRange>,
}

impl PolicyMatch {
    pub fn new(
        app: Option<AppMatcher>,
        domain: Option<DomainMatcher>,
        cidr: Option<Cidr>,
        network: Option<NetworkMatcher>,
        mut transports: Vec<Transport>,
        mut ports: Vec<PortRange>,
    ) -> Result<Self, ModelError> {
        if domain.is_some() && cidr.is_some() {
            return Err(ModelError::AmbiguousDestinationMatcher);
        }
        let has_non_network_dimension = app.is_some()
            || domain.is_some()
            || cidr.is_some()
            || !transports.is_empty()
            || !ports.is_empty();
        if network.is_some() && has_non_network_dimension {
            return Err(ModelError::NetworkMatcherCannotBeCombined);
        }

        transports.sort_unstable();
        transports.dedup();
        ports.sort_unstable();
        validate_port_ranges(&ports)?;

        Ok(Self {
            app,
            domain,
            cidr,
            network,
            transports,
            ports,
        })
    }

    #[must_use]
    pub fn global() -> Self {
        Self {
            app: None,
            domain: None,
            cidr: None,
            network: None,
            transports: Vec::new(),
            ports: Vec::new(),
        }
    }

    #[must_use]
    pub const fn app(&self) -> Option<&AppMatcher> {
        self.app.as_ref()
    }

    #[must_use]
    pub const fn domain(&self) -> Option<&DomainMatcher> {
        self.domain.as_ref()
    }

    #[must_use]
    pub const fn cidr(&self) -> Option<Cidr> {
        self.cidr
    }

    #[must_use]
    pub const fn network(&self) -> Option<&NetworkMatcher> {
        self.network.as_ref()
    }

    #[must_use]
    pub fn transports(&self) -> &[Transport] {
        &self.transports
    }

    #[must_use]
    pub fn ports(&self) -> &[PortRange] {
        &self.ports
    }

    #[must_use]
    pub fn matches_transport_and_port(&self, transport: Transport, port: u16) -> bool {
        let transport_matches = self.transports.is_empty() || self.transports.contains(&transport);
        let port_matches =
            self.ports.is_empty() || self.ports.iter().any(|range| range.contains(port));
        transport_matches && port_matches
    }
}

pub trait PolicyValidation {
    fn validate_for_source(&self, source_kind: PolicySourceKind) -> Result<(), ModelError>;
}

impl PolicyValidation for PolicyMatch {
    fn validate_for_source(&self, source_kind: PolicySourceKind) -> Result<(), ModelError> {
        match source_kind {
            PolicySourceKind::AppDestination => {
                if self.app.is_none()
                    || (self.domain.is_none() && self.cidr.is_none())
                    || self.network.is_some()
                {
                    return Err(ModelError::AppDestinationMatcherIncomplete);
                }
            }
            PolicySourceKind::App => {
                if self.app.is_none()
                    || self.domain.is_some()
                    || self.cidr.is_some()
                    || self.network.is_some()
                    || !self.transports.is_empty()
                    || !self.ports.is_empty()
                {
                    return Err(ModelError::AppMatcherHasExtraDimensions);
                }
            }
            PolicySourceKind::Site => {
                if self.app.is_some()
                    || self.domain.is_none()
                    || self.cidr.is_some()
                    || self.network.is_some()
                {
                    return Err(ModelError::SiteMatcherInvalid);
                }
            }
            PolicySourceKind::Cidr => {
                if self.app.is_some()
                    || self.domain.is_some()
                    || self.cidr.is_none()
                    || self.network.is_some()
                {
                    return Err(ModelError::CidrMatcherInvalid);
                }
            }
            PolicySourceKind::Network => {
                if self.app.is_some()
                    || self.domain.is_some()
                    || self.cidr.is_some()
                    || self.network.is_none()
                    || !self.transports.is_empty()
                    || !self.ports.is_empty()
                {
                    return Err(ModelError::NetworkMatcherInvalid);
                }
            }
            PolicySourceKind::System | PolicySourceKind::BuiltIn => {
                if self.network.is_some() {
                    return Err(ModelError::GlobalRuleHasNetworkMatcher);
                }
            }
            PolicySourceKind::Adapter => {
                if self.app.is_none() && self.domain.is_none() && self.cidr.is_none() {
                    return Err(ModelError::AdapterMatcherInvalid);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    id: PolicyId,
    display_name: String,
    source_kind: PolicySourceKind,
    matcher: PolicyMatch,
    decision: DecisionSpec,
    priority: i32,
    enabled: bool,
    origin: PolicyOrigin,
    revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyMetadata {
    source_kind: PolicySourceKind,
    priority: i32,
    origin: PolicyOrigin,
    revision: u64,
}

impl PolicyMetadata {
    #[must_use]
    pub const fn new(
        source_kind: PolicySourceKind,
        priority: i32,
        origin: PolicyOrigin,
        revision: u64,
    ) -> Self {
        Self {
            source_kind,
            priority,
            origin,
            revision,
        }
    }
}

impl Policy {
    pub fn new(
        id: PolicyId,
        display_name: impl Into<String>,
        matcher: PolicyMatch,
        decision: DecisionSpec,
        metadata: PolicyMetadata,
    ) -> Result<Self, ModelError> {
        let display_name = display_name.into();
        if display_name.is_empty() || display_name.trim() != display_name {
            return Err(ModelError::EmptyPolicyDisplayName);
        }
        if display_name.len() > MAX_POLICY_DISPLAY_NAME_LENGTH {
            return Err(ModelError::PolicyDisplayNameTooLong);
        }
        if display_name.chars().any(char::is_control) {
            return Err(ModelError::InvalidPolicyDisplayName);
        }
        validate_origin(metadata.source_kind, metadata.origin)?;
        matcher.validate_for_source(metadata.source_kind)?;

        Ok(Self {
            id,
            display_name,
            source_kind: metadata.source_kind,
            matcher,
            decision,
            priority: metadata.priority,
            enabled: true,
            origin: metadata.origin,
            revision: metadata.revision,
        })
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    #[must_use]
    pub const fn id(&self) -> &PolicyId {
        &self.id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn source_kind(&self) -> PolicySourceKind {
        self.source_kind
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
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn origin(&self) -> PolicyOrigin {
        self.origin
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

fn validate_port_ranges(ports: &[PortRange]) -> Result<(), ModelError> {
    for adjacent in ports.windows(2) {
        if adjacent[0].last() >= adjacent[1].first() {
            return Err(ModelError::OverlappingPortRanges);
        }
    }
    Ok(())
}

fn validate_origin(source_kind: PolicySourceKind, origin: PolicyOrigin) -> Result<(), ModelError> {
    let is_valid = match source_kind {
        PolicySourceKind::System => origin == PolicyOrigin::System,
        PolicySourceKind::BuiltIn => origin == PolicyOrigin::SignedBuiltIn,
        PolicySourceKind::Adapter => origin == PolicyOrigin::Adapter,
        _ => matches!(origin, PolicyOrigin::User | PolicyOrigin::Subscription),
    };
    if !is_valid {
        return Err(ModelError::InvalidPolicyOrigin);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{DomainMatchKind, Platform};

    use super::*;

    fn must_app_matcher() -> AppMatcher {
        let result = AppMatcher::new(Platform::MacOs, "com.example.app");
        match result {
            Ok(matcher) => matcher,
            Err(error) => panic!("测试应用匹配器创建失败: {error}"),
        }
    }

    fn must_domain_matcher() -> DomainMatcher {
        let result = DomainMatcher::new(DomainMatchKind::Suffix, "example.com");
        match result {
            Ok(matcher) => matcher,
            Err(error) => panic!("测试域名匹配器创建失败: {error}"),
        }
    }

    #[test]
    fn app_destination_requires_both_dimensions() {
        let matcher_result = PolicyMatch::new(
            Some(must_app_matcher()),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let Ok(matcher) = matcher_result else {
            panic!("通用匹配结构应当创建成功: {matcher_result:?}");
        };

        assert!(matches!(
            matcher.validate_for_source(PolicySourceKind::AppDestination),
            Err(ModelError::AppDestinationMatcherIncomplete)
        ));
    }

    #[test]
    fn port_ranges_are_sorted_and_overlap_is_rejected() {
        let first_result = PortRange::new(443, 500);
        let second_result = PortRange::new(400, 450);
        let (Ok(first), Ok(second)) = (first_result, second_result) else {
            panic!("测试端口范围创建失败");
        };

        assert!(matches!(
            PolicyMatch::new(
                None,
                Some(must_domain_matcher()),
                None,
                None,
                Vec::new(),
                vec![first, second],
            ),
            Err(ModelError::OverlappingPortRanges)
        ));
    }

    #[test]
    fn network_matcher_cannot_hide_other_dimensions() {
        let profile_result = NetworkProfileId::new("office");
        let Ok(profile_id) = profile_result else {
            panic!("测试网络配置档标识创建失败: {profile_result:?}");
        };

        assert!(matches!(
            PolicyMatch::new(
                Some(must_app_matcher()),
                None,
                None,
                Some(NetworkMatcher::new(profile_id)),
                Vec::new(),
                Vec::new(),
            ),
            Err(ModelError::NetworkMatcherCannotBeCombined)
        ));
    }

    #[test]
    fn untrusted_origin_cannot_claim_system_precedence() {
        assert!(matches!(
            validate_origin(PolicySourceKind::System, PolicyOrigin::User),
            Err(ModelError::InvalidPolicyOrigin)
        ));
        assert!(matches!(
            validate_origin(PolicySourceKind::BuiltIn, PolicyOrigin::Subscription),
            Err(ModelError::InvalidPolicyOrigin)
        ));
    }
}
