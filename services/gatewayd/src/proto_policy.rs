use std::net::IpAddr;

use nonproxy_model::{
    AppMatcher, Cidr, DecisionSpec, DomainMatchKind, DomainMatcher, FailureMode, NetworkMatcher,
    NetworkProfileId, OutboundGroupId, OutboundId, Platform, Policy, PolicyId, PolicyMatch,
    PolicyMetadata, PolicyOrigin, PolicySourceKind, PortRange, ProxyTarget, RouteAction, Transport,
};
use nonproxy_proto::{
    common::v1 as common_proto,
    policy::v1::{
        self as policy_proto, AppMatcher as ProtoAppMatcher, CidrMatcher as ProtoCidrMatcher,
        DecisionSpec as ProtoDecisionSpec, DomainMatcher as ProtoDomainMatcher,
        NetworkMatcher as ProtoNetworkMatcher, Policy as ProtoPolicy,
        PolicyMatch as ProtoPolicyMatch, PortRange as ProtoPortRange,
    },
};

use crate::GatewayError;

pub fn policy_from_proto(value: ProtoPolicy) -> Result<Policy, GatewayError> {
    if value.revision == 0 {
        return Err(GatewayError::InvalidContract("策略 revision 必须大于零"));
    }
    let source_kind = source_from_proto(value.source_kind)?;
    let origin = origin_from_proto(value.origin)?;
    let matcher = matcher_from_proto(
        value
            .r#match
            .ok_or(GatewayError::InvalidContract("策略缺少 match"))?,
    )?;
    let decision = decision_from_proto(
        value
            .decision
            .ok_or(GatewayError::InvalidContract("策略缺少 decision"))?,
    )?;
    let mut policy = Policy::new(
        PolicyId::new(value.id)?,
        value.display_name,
        matcher,
        decision,
        PolicyMetadata::new(source_kind, value.priority, origin, value.revision),
    )?;
    if !value.enabled {
        policy = policy.disabled();
    }
    Ok(policy)
}

#[must_use]
pub fn policy_to_proto(value: &Policy) -> ProtoPolicy {
    ProtoPolicy {
        id: value.id().as_str().to_owned(),
        display_name: value.display_name().to_owned(),
        source_kind: source_to_proto(value.source_kind()) as i32,
        r#match: Some(matcher_to_proto(value.matcher())),
        decision: Some(decision_to_proto(value.decision())),
        priority: value.priority(),
        enabled: value.enabled(),
        origin: origin_to_proto(value.origin()) as i32,
        revision: value.revision(),
        created_at: None,
        updated_at: None,
    }
}

fn matcher_from_proto(value: ProtoPolicyMatch) -> Result<PolicyMatch, GatewayError> {
    let app = value.app.map(app_from_proto).transpose()?;
    let domain = value.domain.map(domain_from_proto).transpose()?;
    let cidr = value.cidr.map(cidr_from_proto).transpose()?;
    let network = value.network.map(network_from_proto).transpose()?;
    let transports = value
        .transports
        .into_iter()
        .map(transport_from_proto)
        .collect::<Result<Vec<_>, _>>()?;
    let ports = value
        .ports
        .into_iter()
        .map(|range| {
            let first = u16::try_from(range.first)
                .map_err(|_| GatewayError::InvalidContract("端口范围超出 u16"))?;
            let last = u16::try_from(range.last)
                .map_err(|_| GatewayError::InvalidContract("端口范围超出 u16"))?;
            PortRange::new(first, last).map_err(GatewayError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    PolicyMatch::new(app, domain, cidr, network, transports, ports).map_err(GatewayError::from)
}

fn matcher_to_proto(value: &PolicyMatch) -> ProtoPolicyMatch {
    ProtoPolicyMatch {
        app: value.app().map(app_to_proto),
        domain: value.domain().map(domain_to_proto),
        cidr: value.cidr().map(cidr_to_proto),
        network: value.network().map(network_to_proto),
        transports: value
            .transports()
            .iter()
            .copied()
            .map(|item| transport_to_proto(item) as i32)
            .collect(),
        ports: value
            .ports()
            .iter()
            .map(|range| ProtoPortRange {
                first: u32::from(range.first()),
                last: u32::from(range.last()),
            })
            .collect(),
    }
}

fn app_from_proto(value: ProtoAppMatcher) -> Result<AppMatcher, GatewayError> {
    let platform = match common_proto::Platform::try_from(value.platform) {
        Ok(common_proto::Platform::Macos) => Platform::MacOs,
        Ok(common_proto::Platform::Windows) => Platform::Windows,
        _ => return Err(GatewayError::InvalidContract("应用平台无效")),
    };
    let mut matcher = AppMatcher::new(platform, value.stable_id)?;
    if !value.signer_id.is_empty() {
        matcher = matcher.with_signer_id(value.signer_id)?;
    }
    Ok(matcher.include_helpers(value.include_helpers))
}

fn app_to_proto(value: &AppMatcher) -> ProtoAppMatcher {
    ProtoAppMatcher {
        platform: match value.platform() {
            Platform::MacOs => common_proto::Platform::Macos as i32,
            Platform::Windows => common_proto::Platform::Windows as i32,
        },
        stable_id: value.stable_id().to_owned(),
        signer_id: value.signer_id().unwrap_or_default().to_owned(),
        include_helpers: value.includes_helpers(),
    }
}

fn domain_from_proto(value: ProtoDomainMatcher) -> Result<DomainMatcher, GatewayError> {
    let kind = match policy_proto::DomainMatchKind::try_from(value.kind) {
        Ok(policy_proto::DomainMatchKind::Exact) => DomainMatchKind::Exact,
        Ok(policy_proto::DomainMatchKind::Suffix) => DomainMatchKind::Suffix,
        Ok(policy_proto::DomainMatchKind::RegistrableDomain) => DomainMatchKind::RegistrableDomain,
        _ => return Err(GatewayError::InvalidContract("域名匹配类型无效")),
    };
    DomainMatcher::new(kind, &value.ascii_pattern).map_err(GatewayError::from)
}

fn domain_to_proto(value: &DomainMatcher) -> ProtoDomainMatcher {
    ProtoDomainMatcher {
        kind: match value.kind() {
            DomainMatchKind::Exact => policy_proto::DomainMatchKind::Exact as i32,
            DomainMatchKind::Suffix => policy_proto::DomainMatchKind::Suffix as i32,
            DomainMatchKind::RegistrableDomain => {
                policy_proto::DomainMatchKind::RegistrableDomain as i32
            }
        },
        ascii_pattern: value.pattern().as_ascii().to_owned(),
    }
}

fn cidr_from_proto(value: ProtoCidrMatcher) -> Result<Cidr, GatewayError> {
    let address = value
        .network
        .parse::<IpAddr>()
        .map_err(|_| GatewayError::InvalidContract("CIDR 网络地址无效"))?;
    let prefix = u8::try_from(value.prefix_length)
        .map_err(|_| GatewayError::InvalidContract("CIDR 前缀长度无效"))?;
    Cidr::new(address, prefix).map_err(GatewayError::from)
}

fn cidr_to_proto(value: Cidr) -> ProtoCidrMatcher {
    ProtoCidrMatcher {
        network: value.network().to_string(),
        prefix_length: u32::from(value.prefix_length()),
    }
}

fn network_from_proto(value: ProtoNetworkMatcher) -> Result<NetworkMatcher, GatewayError> {
    Ok(NetworkMatcher::new(NetworkProfileId::new(
        value.profile_id,
    )?))
}

fn network_to_proto(value: &NetworkMatcher) -> ProtoNetworkMatcher {
    ProtoNetworkMatcher {
        profile_id: value.profile_id().as_str().to_owned(),
    }
}

pub fn decision_from_proto(value: ProtoDecisionSpec) -> Result<DecisionSpec, GatewayError> {
    let action = match common_proto::RouteAction::try_from(value.action) {
        Ok(common_proto::RouteAction::Direct) => RouteAction::Direct,
        Ok(common_proto::RouteAction::Proxy) => RouteAction::Proxy,
        Ok(common_proto::RouteAction::Block) => RouteAction::Block,
        _ => return Err(GatewayError::InvalidContract("路由动作无效")),
    };
    let failure_mode = match common_proto::FailureMode::try_from(value.failure_mode) {
        Ok(common_proto::FailureMode::Closed) => FailureMode::Closed,
        Ok(common_proto::FailureMode::Open) => FailureMode::Open,
        _ => return Err(GatewayError::InvalidContract("失败模式无效")),
    };
    let proxy_target = match (
        value.outbound_id.is_empty(),
        value.outbound_group_id.is_empty(),
    ) {
        (false, true) => Some(ProxyTarget::Outbound(OutboundId::new(value.outbound_id)?)),
        (true, false) => Some(ProxyTarget::Group(OutboundGroupId::new(
            value.outbound_group_id,
        )?)),
        (true, true) => None,
        (false, false) => {
            return Err(GatewayError::InvalidContract(
                "代理决策不能同时指定出口和出口组",
            ));
        }
    };
    DecisionSpec::new_with_target(action, proxy_target, failure_mode).map_err(GatewayError::from)
}

#[must_use]
pub fn decision_to_proto(value: &DecisionSpec) -> ProtoDecisionSpec {
    let (outbound_id, outbound_group_id) = match value.proxy_target() {
        Some(ProxyTarget::Outbound(value)) => (value.as_str().to_owned(), String::new()),
        Some(ProxyTarget::Group(value)) => (String::new(), value.as_str().to_owned()),
        None => (String::new(), String::new()),
    };
    ProtoDecisionSpec {
        action: match value.action() {
            RouteAction::Direct => common_proto::RouteAction::Direct as i32,
            RouteAction::Proxy => common_proto::RouteAction::Proxy as i32,
            RouteAction::Block => common_proto::RouteAction::Block as i32,
        },
        failure_mode: match value.failure_mode() {
            FailureMode::Closed => common_proto::FailureMode::Closed as i32,
            FailureMode::Open => common_proto::FailureMode::Open as i32,
        },
        outbound_id,
        outbound_group_id,
    }
}

pub fn transport_from_proto(value: i32) -> Result<Transport, GatewayError> {
    match common_proto::TransportProtocol::try_from(value) {
        Ok(common_proto::TransportProtocol::Tcp) => Ok(Transport::Tcp),
        Ok(common_proto::TransportProtocol::Udp) => Ok(Transport::Udp),
        _ => Err(GatewayError::InvalidContract("传输协议无效")),
    }
}

#[must_use]
pub const fn transport_to_proto(value: Transport) -> common_proto::TransportProtocol {
    match value {
        Transport::Tcp => common_proto::TransportProtocol::Tcp,
        Transport::Udp => common_proto::TransportProtocol::Udp,
    }
}

fn source_from_proto(value: i32) -> Result<PolicySourceKind, GatewayError> {
    match policy_proto::PolicySourceKind::try_from(value) {
        Ok(policy_proto::PolicySourceKind::System) => Ok(PolicySourceKind::System),
        Ok(policy_proto::PolicySourceKind::AppDestination) => Ok(PolicySourceKind::AppDestination),
        Ok(policy_proto::PolicySourceKind::App) => Ok(PolicySourceKind::App),
        Ok(policy_proto::PolicySourceKind::Site) => Ok(PolicySourceKind::Site),
        Ok(policy_proto::PolicySourceKind::Network) => Ok(PolicySourceKind::Network),
        Ok(policy_proto::PolicySourceKind::BuiltIn) => Ok(PolicySourceKind::BuiltIn),
        Ok(policy_proto::PolicySourceKind::Cidr) => Ok(PolicySourceKind::Cidr),
        Ok(policy_proto::PolicySourceKind::Adapter) => Ok(PolicySourceKind::Adapter),
        _ => Err(GatewayError::InvalidContract("策略来源类型无效")),
    }
}

const fn source_to_proto(value: PolicySourceKind) -> policy_proto::PolicySourceKind {
    match value {
        PolicySourceKind::System => policy_proto::PolicySourceKind::System,
        PolicySourceKind::AppDestination => policy_proto::PolicySourceKind::AppDestination,
        PolicySourceKind::App => policy_proto::PolicySourceKind::App,
        PolicySourceKind::Site => policy_proto::PolicySourceKind::Site,
        PolicySourceKind::Network => policy_proto::PolicySourceKind::Network,
        PolicySourceKind::BuiltIn => policy_proto::PolicySourceKind::BuiltIn,
        PolicySourceKind::Cidr => policy_proto::PolicySourceKind::Cidr,
        PolicySourceKind::Adapter => policy_proto::PolicySourceKind::Adapter,
    }
}

fn origin_from_proto(value: i32) -> Result<PolicyOrigin, GatewayError> {
    match policy_proto::PolicyOrigin::try_from(value) {
        Ok(policy_proto::PolicyOrigin::System) => Ok(PolicyOrigin::System),
        Ok(policy_proto::PolicyOrigin::User) => Ok(PolicyOrigin::User),
        Ok(policy_proto::PolicyOrigin::SignedBuiltIn) => Ok(PolicyOrigin::SignedBuiltIn),
        Ok(policy_proto::PolicyOrigin::Subscription) => Ok(PolicyOrigin::Subscription),
        Ok(policy_proto::PolicyOrigin::Adapter) => Ok(PolicyOrigin::Adapter),
        _ => Err(GatewayError::InvalidContract("策略来源信任级别无效")),
    }
}

const fn origin_to_proto(value: PolicyOrigin) -> policy_proto::PolicyOrigin {
    match value {
        PolicyOrigin::System => policy_proto::PolicyOrigin::System,
        PolicyOrigin::User => policy_proto::PolicyOrigin::User,
        PolicyOrigin::SignedBuiltIn => policy_proto::PolicyOrigin::SignedBuiltIn,
        PolicyOrigin::Subscription => policy_proto::PolicyOrigin::Subscription,
        PolicyOrigin::Adapter => policy_proto::PolicyOrigin::Adapter,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy_decision(outbound_id: &str, outbound_group_id: &str) -> ProtoDecisionSpec {
        ProtoDecisionSpec {
            action: common_proto::RouteAction::Proxy as i32,
            outbound_id: outbound_id.to_owned(),
            failure_mode: common_proto::FailureMode::Closed as i32,
            outbound_group_id: outbound_group_id.to_owned(),
        }
    }

    #[test]
    fn rejects_ambiguous_proxy_target() {
        let error = decision_from_proto(proxy_decision("primary", "automatic"))
            .expect_err("simultaneous outbound and group targets must be rejected");

        assert!(matches!(error, GatewayError::InvalidContract(_)));
    }

    #[test]
    fn round_trips_group_target_without_legacy_outbound_field() {
        let decision = decision_from_proto(proxy_decision("", "automatic"))
            .expect("group target should be accepted");

        let encoded = decision_to_proto(&decision);

        assert_eq!(encoded.outbound_group_id, "automatic");
        assert!(encoded.outbound_id.is_empty());
    }
}
