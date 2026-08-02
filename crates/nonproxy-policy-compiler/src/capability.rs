use std::collections::BTreeMap;

use nonproxy_model::{
    DecisionSpec, IpFamily, OutboundGroupId, OutboundGroupSpec, OutboundId, Policy, ProxyTarget,
    RouteAction, Transport,
};
use nonproxy_policy::OutboundCapabilities;

use crate::{CompileError, PolicyConflict};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileCapabilities {
    app_matching: bool,
    domain_matching: bool,
    cidr_matching: bool,
    tcp: bool,
    udp: bool,
    ipv4: bool,
    ipv6: bool,
    outbounds: BTreeMap<OutboundId, OutboundCapabilities>,
    outbound_groups: BTreeMap<OutboundGroupId, OutboundGroupSpec>,
    outbound_group_capabilities: BTreeMap<OutboundGroupId, OutboundCapabilities>,
}

impl CompileCapabilities {
    #[must_use]
    pub const fn new(
        app_matching: bool,
        domain_matching: bool,
        cidr_matching: bool,
        transport: OutboundCapabilities,
    ) -> Self {
        Self {
            app_matching,
            domain_matching,
            cidr_matching,
            tcp: transport.supports_transport(Transport::Tcp),
            udp: transport.supports_transport(Transport::Udp),
            ipv4: transport.supports_family(IpFamily::Ipv4),
            ipv6: transport.supports_family(IpFamily::Ipv6),
            outbounds: BTreeMap::new(),
            outbound_groups: BTreeMap::new(),
            outbound_group_capabilities: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn full() -> Self {
        Self::new(true, true, true, OutboundCapabilities::full())
    }

    #[must_use]
    pub fn with_outbound(
        mut self,
        outbound_id: OutboundId,
        capabilities: OutboundCapabilities,
    ) -> Self {
        self.outbounds.insert(outbound_id, capabilities);
        self
    }

    pub fn with_outbound_group(mut self, group: OutboundGroupSpec) -> Result<Self, CompileError> {
        let mut members = group.members().iter();
        let first = members
            .next()
            .and_then(|member| self.outbounds.get(member).copied())
            .ok_or(CompileError::OutboundGroupMemberUnknown)?;
        let capabilities = members.try_fold(first, |intersection, member| {
            self.outbounds
                .get(member)
                .copied()
                .map(|capabilities| intersection.intersection(capabilities))
                .ok_or(CompileError::OutboundGroupMemberUnknown)
        })?;
        let id = group.id().clone();
        self.outbound_groups.insert(id.clone(), group);
        self.outbound_group_capabilities.insert(id, capabilities);
        Ok(self)
    }

    pub(crate) fn validate_policy(&self, policy: &Policy, conflicts: &mut Vec<PolicyConflict>) {
        let matcher = policy.matcher();
        if matcher.app().is_some() && !self.app_matching {
            conflicts.push(PolicyConflict::for_policy(
                "NP_POLICY_CAPABILITY_APP_UNSUPPORTED",
                "目标平台不支持按应用识别",
                policy.id().clone(),
            ));
        }
        if matcher.domain().is_some() && !self.domain_matching {
            conflicts.push(PolicyConflict::for_policy(
                "NP_POLICY_CAPABILITY_DOMAIN_UNSUPPORTED",
                "目标平台不支持按域名识别",
                policy.id().clone(),
            ));
        }
        if matcher.cidr().is_some() && !self.cidr_matching {
            conflicts.push(PolicyConflict::for_policy(
                "NP_POLICY_CAPABILITY_CIDR_UNSUPPORTED",
                "目标平台不支持按 CIDR 识别",
                policy.id().clone(),
            ));
        }
        self.validate_transports(policy, conflicts);
        self.validate_families(policy, conflicts);
        self.validate_decision(policy.decision(), Some(policy), conflicts);
    }

    pub(crate) fn validate_target(&self, conflicts: &mut Vec<PolicyConflict>) {
        if !self.tcp && !self.udp {
            conflicts.push(PolicyConflict::global(
                "NP_POLICY_TARGET_TRANSPORT_EMPTY",
                "目标平台至少必须支持一种传输协议",
            ));
        }
        if !self.ipv4 && !self.ipv6 {
            conflicts.push(PolicyConflict::global(
                "NP_POLICY_TARGET_IP_FAMILY_EMPTY",
                "目标平台至少必须支持一种 IP 地址族",
            ));
        }
    }

    pub(crate) fn validate_default(
        &self,
        decision: &DecisionSpec,
        conflicts: &mut Vec<PolicyConflict>,
    ) {
        self.validate_decision(decision, None, conflicts);
    }

    pub const fn outbounds(&self) -> &BTreeMap<OutboundId, OutboundCapabilities> {
        &self.outbounds
    }

    pub const fn outbound_groups(&self) -> &BTreeMap<OutboundGroupId, OutboundGroupSpec> {
        &self.outbound_groups
    }

    pub const fn outbound_group_capabilities(
        &self,
    ) -> &BTreeMap<OutboundGroupId, OutboundCapabilities> {
        &self.outbound_group_capabilities
    }

    #[must_use]
    pub const fn supports_app_matching(&self) -> bool {
        self.app_matching
    }

    #[must_use]
    pub const fn supports_domain_matching(&self) -> bool {
        self.domain_matching
    }

    #[must_use]
    pub const fn supports_cidr_matching(&self) -> bool {
        self.cidr_matching
    }

    #[must_use]
    pub const fn supports_transport(&self, transport: Transport) -> bool {
        match transport {
            Transport::Tcp => self.tcp,
            Transport::Udp => self.udp,
        }
    }

    #[must_use]
    pub const fn supports_family(&self, family: IpFamily) -> bool {
        match family {
            IpFamily::Ipv4 => self.ipv4,
            IpFamily::Ipv6 => self.ipv6,
        }
    }

    fn validate_transports(&self, policy: &Policy, conflicts: &mut Vec<PolicyConflict>) {
        for transport in policy.matcher().transports() {
            let supported = match transport {
                Transport::Tcp => self.tcp,
                Transport::Udp => self.udp,
            };
            if !supported {
                conflicts.push(PolicyConflict::for_policy(
                    "NP_POLICY_CAPABILITY_TRANSPORT_UNSUPPORTED",
                    "规则包含目标平台不支持的传输协议",
                    policy.id().clone(),
                ));
            }
        }
    }

    fn validate_families(&self, policy: &Policy, conflicts: &mut Vec<PolicyConflict>) {
        let Some(cidr) = policy.matcher().cidr() else {
            return;
        };
        let supported = match cidr.network() {
            std::net::IpAddr::V4(_) => self.ipv4,
            std::net::IpAddr::V6(_) => self.ipv6,
        };
        if !supported {
            conflicts.push(PolicyConflict::for_policy(
                "NP_POLICY_CAPABILITY_IP_FAMILY_UNSUPPORTED",
                "规则包含目标平台不支持的 IP 地址族",
                policy.id().clone(),
            ));
        }
    }

    fn validate_decision(
        &self,
        decision: &DecisionSpec,
        policy: Option<&Policy>,
        conflicts: &mut Vec<PolicyConflict>,
    ) {
        if decision.action() != RouteAction::Proxy {
            return;
        }
        let Some(target) = decision.proxy_target() else {
            return;
        };
        let capabilities = match target {
            ProxyTarget::Outbound(outbound_id) => match self.outbounds.get(outbound_id).copied() {
                Some(capabilities) => capabilities,
                None => {
                    push_decision_conflict(
                        conflicts,
                        policy,
                        "NP_POLICY_OUTBOUND_UNKNOWN",
                        "代理决策引用了未注册的出口",
                    );
                    return;
                }
            },
            ProxyTarget::Group(group_id) => {
                match self.outbound_group_capabilities.get(group_id).copied() {
                    Some(capabilities) => capabilities,
                    None => {
                        push_decision_conflict(
                            conflicts,
                            policy,
                            "NP_POLICY_OUTBOUND_GROUP_UNKNOWN",
                            "代理决策引用了未注册的出口组",
                        );
                        return;
                    }
                }
            }
        };

        if required_transports(self, policy)
            .into_iter()
            .any(|transport| !capabilities.supports_transport(transport))
        {
            push_decision_conflict(
                conflicts,
                policy,
                "NP_POLICY_OUTBOUND_TRANSPORT_UNSUPPORTED",
                "代理出口不支持规则所需的传输协议",
            );
        }
        if required_families(self, policy)
            .into_iter()
            .any(|family| !capabilities.supports_family(family))
        {
            push_decision_conflict(
                conflicts,
                policy,
                "NP_POLICY_OUTBOUND_IP_FAMILY_UNSUPPORTED",
                "代理出口不支持规则所需的 IP 地址族",
            );
        }
    }
}

fn required_transports(
    capabilities: &CompileCapabilities,
    policy: Option<&Policy>,
) -> Vec<Transport> {
    if let Some(transports) = policy
        .map(Policy::matcher)
        .map(|matcher| matcher.transports())
        .filter(|transports| !transports.is_empty())
    {
        return transports.to_vec();
    }
    [
        (capabilities.tcp, Transport::Tcp),
        (capabilities.udp, Transport::Udp),
    ]
    .into_iter()
    .filter_map(|(enabled, transport)| enabled.then_some(transport))
    .collect()
}

fn required_families(capabilities: &CompileCapabilities, policy: Option<&Policy>) -> Vec<IpFamily> {
    if let Some(cidr) = policy.and_then(|value| value.matcher().cidr()) {
        let family = match cidr.network() {
            std::net::IpAddr::V4(_) => IpFamily::Ipv4,
            std::net::IpAddr::V6(_) => IpFamily::Ipv6,
        };
        return vec![family];
    }
    [
        (capabilities.ipv4, IpFamily::Ipv4),
        (capabilities.ipv6, IpFamily::Ipv6),
    ]
    .into_iter()
    .filter_map(|(enabled, family)| enabled.then_some(family))
    .collect()
}

fn push_decision_conflict(
    conflicts: &mut Vec<PolicyConflict>,
    policy: Option<&Policy>,
    code: &'static str,
    message: &'static str,
) {
    match policy {
        Some(policy) => conflicts.push(PolicyConflict::for_policy(
            code,
            message,
            policy.id().clone(),
        )),
        None => conflicts.push(PolicyConflict::global(code, message)),
    }
}
