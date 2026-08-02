use std::collections::BTreeMap;

use nonproxy_model::{
    DecisionSpec, DomainName, NetworkFingerprint, NetworkProfileId, OutboundGroupId,
    OutboundGroupSpec, OutboundId, RuntimeRoutingOverride,
};

use crate::{
    CompiledRule, OutboundCapabilities, RuleTier,
    index::{
        AppDestinationRuleIndex, AppRuleIndex, CidrRuleIndex, DomainRuleIndex, NetworkRuleIndex,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotMetadata {
    schema_version: u32,
    snapshot_version: u64,
    created_at_unix_ms: u64,
    content_hash: [u8; 32],
    policy_count: usize,
}

impl SnapshotMetadata {
    #[must_use]
    pub const fn new(
        schema_version: u32,
        snapshot_version: u64,
        created_at_unix_ms: u64,
        content_hash: [u8; 32],
        policy_count: usize,
    ) -> Self {
        Self {
            schema_version,
            snapshot_version,
            created_at_unix_ms,
            content_hash,
            policy_count,
        }
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn snapshot_version(&self) -> u64 {
        self.snapshot_version
    }

    #[must_use]
    pub const fn created_at_unix_ms(&self) -> u64 {
        self.created_at_unix_ms
    }

    #[must_use]
    pub const fn content_hash(&self) -> &[u8; 32] {
        &self.content_hash
    }

    #[must_use]
    pub const fn policy_count(&self) -> usize {
        self.policy_count
    }
}

#[derive(Clone, Debug)]
pub struct CompiledOutboundCatalog {
    outbounds: BTreeMap<OutboundId, OutboundCapabilities>,
    groups: BTreeMap<OutboundGroupId, OutboundGroupSpec>,
    group_capabilities: BTreeMap<OutboundGroupId, OutboundCapabilities>,
}

impl CompiledOutboundCatalog {
    #[must_use]
    pub fn new(
        outbounds: BTreeMap<OutboundId, OutboundCapabilities>,
        groups: BTreeMap<OutboundGroupId, OutboundGroupSpec>,
        group_capabilities: BTreeMap<OutboundGroupId, OutboundCapabilities>,
    ) -> Self {
        Self {
            outbounds,
            groups,
            group_capabilities,
        }
    }

    #[must_use]
    pub const fn outbounds(&self) -> &BTreeMap<OutboundId, OutboundCapabilities> {
        &self.outbounds
    }

    #[must_use]
    pub const fn groups(&self) -> &BTreeMap<OutboundGroupId, OutboundGroupSpec> {
        &self.groups
    }

    #[must_use]
    pub const fn group_capabilities(&self) -> &BTreeMap<OutboundGroupId, OutboundCapabilities> {
        &self.group_capabilities
    }
}

#[derive(Clone, Debug)]
pub struct CompiledPolicySnapshot {
    metadata: SnapshotMetadata,
    default_decision: DecisionSpec,
    outbound_catalog: CompiledOutboundCatalog,
    network_profiles: BTreeMap<NetworkProfileId, NetworkFingerprint>,
    runtime_override: Option<RuntimeRoutingOverride>,
    system_rules: Vec<CompiledRule>,
    app_destination_rules: AppDestinationRuleIndex,
    app_rules: AppRuleIndex,
    domain_rules: DomainRuleIndex,
    cidr_rules: CidrRuleIndex,
    network_rules: NetworkRuleIndex,
    built_in_rules: Vec<CompiledRule>,
}

impl CompiledPolicySnapshot {
    #[must_use]
    pub fn from_compiled_rules(
        metadata: SnapshotMetadata,
        default_decision: DecisionSpec,
        outbound_catalog: CompiledOutboundCatalog,
        network_profiles: BTreeMap<NetworkProfileId, NetworkFingerprint>,
        runtime_override: Option<RuntimeRoutingOverride>,
        rules: Vec<CompiledRule>,
    ) -> Self {
        let mut snapshot = Self {
            metadata,
            default_decision,
            outbound_catalog,
            network_profiles,
            runtime_override,
            system_rules: Vec::new(),
            app_destination_rules: AppDestinationRuleIndex::default(),
            app_rules: AppRuleIndex::default(),
            domain_rules: DomainRuleIndex::default(),
            cidr_rules: CidrRuleIndex::default(),
            network_rules: NetworkRuleIndex::default(),
            built_in_rules: Vec::new(),
        };
        for rule in rules {
            snapshot.insert(rule);
        }
        snapshot
    }

    #[must_use]
    pub const fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }

    #[must_use]
    pub const fn default_decision(&self) -> &DecisionSpec {
        &self.default_decision
    }

    #[must_use]
    pub const fn outbound_capabilities(&self) -> &BTreeMap<OutboundId, OutboundCapabilities> {
        self.outbound_catalog.outbounds()
    }

    #[must_use]
    pub const fn outbound_groups(&self) -> &BTreeMap<OutboundGroupId, OutboundGroupSpec> {
        self.outbound_catalog.groups()
    }

    #[must_use]
    pub const fn outbound_group_capabilities(
        &self,
    ) -> &BTreeMap<OutboundGroupId, OutboundCapabilities> {
        self.outbound_catalog.group_capabilities()
    }

    #[must_use]
    pub const fn network_profiles(&self) -> &BTreeMap<NetworkProfileId, NetworkFingerprint> {
        &self.network_profiles
    }

    #[must_use]
    pub const fn runtime_override(&self) -> Option<&RuntimeRoutingOverride> {
        self.runtime_override.as_ref()
    }

    #[must_use]
    pub fn requires_domain_identity(&self, domain: &DomainName) -> bool {
        self.domain_rules.contains_domain(domain)
            || self.app_destination_rules.contains_domain(domain)
            || self
                .system_rules
                .iter()
                .chain(&self.built_in_rules)
                .any(|rule| {
                    rule.matcher()
                        .domain()
                        .is_some_and(|matcher| matcher.matches(domain))
                })
    }

    fn insert(&mut self, rule: CompiledRule) {
        match rule.tier() {
            RuleTier::System => self.system_rules.push(rule),
            RuleTier::AppDestination => self.app_destination_rules.insert(rule),
            RuleTier::App => self.app_rules.insert(rule),
            RuleTier::Destination => {
                if rule.matcher().domain().is_some() {
                    self.domain_rules.insert(rule);
                } else {
                    self.cidr_rules.insert(rule);
                }
            }
            RuleTier::Network => self.network_rules.insert(rule),
            RuleTier::BuiltIn => self.built_in_rules.push(rule),
        }
    }

    pub(crate) fn system_rules(&self) -> &[CompiledRule] {
        &self.system_rules
    }

    pub(crate) const fn app_destination_rules(&self) -> &AppDestinationRuleIndex {
        &self.app_destination_rules
    }

    pub(crate) const fn app_rules(&self) -> &AppRuleIndex {
        &self.app_rules
    }

    pub(crate) const fn domain_rules(&self) -> &DomainRuleIndex {
        &self.domain_rules
    }

    pub(crate) const fn cidr_rules(&self) -> &CidrRuleIndex {
        &self.cidr_rules
    }

    pub(crate) const fn network_rules(&self) -> &NetworkRuleIndex {
        &self.network_rules
    }

    pub(crate) fn built_in_rules(&self) -> &[CompiledRule] {
        &self.built_in_rules
    }
}
