#![allow(dead_code)]

use std::net::IpAddr;

use nonproxy_model::{
    AppIdentity, AppMatcher, Cidr, ConnectionContext, DecisionSpec, Destination, DomainMatchKind,
    DomainMatcher, NetworkMatcher, NetworkProfileId, Platform, Policy, PolicyId, PolicyMatch,
    PolicyMetadata, PolicyOrigin, PolicySourceKind, PortRange, Transport,
};

pub fn must_policy(
    id: &str,
    source: PolicySourceKind,
    matcher: PolicyMatch,
    decision: DecisionSpec,
    priority: i32,
) -> Policy {
    let origin = match source {
        PolicySourceKind::System => PolicyOrigin::System,
        PolicySourceKind::BuiltIn => PolicyOrigin::SignedBuiltIn,
        PolicySourceKind::Adapter => PolicyOrigin::Adapter,
        _ => PolicyOrigin::User,
    };
    let id = match PolicyId::new(id) {
        Ok(value) => value,
        Err(error) => panic!("测试策略标识创建失败: {error}"),
    };
    match Policy::new(
        id,
        format!("策略 {source:?}"),
        matcher,
        decision,
        PolicyMetadata::new(source, priority, origin, 1),
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试策略创建失败: {error}"),
    }
}

pub fn app_match(stable_id: &str) -> AppMatcher {
    match AppMatcher::new(Platform::MacOs, stable_id) {
        Ok(value) => value,
        Err(error) => panic!("测试应用匹配器创建失败: {error}"),
    }
}

pub fn app_identity(stable_id: &str) -> AppIdentity {
    match AppIdentity::new(Platform::MacOs, stable_id) {
        Ok(value) => value,
        Err(error) => panic!("测试应用身份创建失败: {error}"),
    }
}

pub fn domain_match(kind: DomainMatchKind, domain: &str) -> DomainMatcher {
    match DomainMatcher::new(kind, domain) {
        Ok(value) => value,
        Err(error) => panic!("测试域名匹配器创建失败: {error}"),
    }
}

pub fn cidr_match(value: &str) -> Cidr {
    match value.parse() {
        Ok(value) => value,
        Err(error) => panic!("测试 CIDR 创建失败: {error}"),
    }
}

pub fn network_match(value: &str) -> NetworkMatcher {
    let id = match NetworkProfileId::new(value) {
        Ok(value) => value,
        Err(error) => panic!("测试网络标识创建失败: {error}"),
    };
    NetworkMatcher::new(id)
}

pub fn matcher(
    app: Option<AppMatcher>,
    domain: Option<DomainMatcher>,
    cidr: Option<Cidr>,
    network: Option<NetworkMatcher>,
    transports: Vec<Transport>,
    ports: Vec<PortRange>,
) -> PolicyMatch {
    match PolicyMatch::new(app, domain, cidr, network, transports, ports) {
        Ok(value) => value,
        Err(error) => panic!("测试策略匹配器创建失败: {error}"),
    }
}

pub fn destination(
    hostname: Option<&str>,
    ip: Option<IpAddr>,
    port: u16,
    transport: Transport,
) -> Destination {
    match Destination::new(hostname, ip, port, transport) {
        Ok(value) => value,
        Err(error) => panic!("测试目标地址创建失败: {error}"),
    }
}

pub fn context(
    stable_id: &str,
    hostname: Option<&str>,
    ip: Option<IpAddr>,
    port: u16,
    transport: Transport,
) -> ConnectionContext {
    ConnectionContext::new(
        app_identity(stable_id),
        destination(hostname, ip, port, transport),
    )
}

pub fn port(first: u16, last: u16) -> PortRange {
    match PortRange::new(first, last) {
        Ok(value) => value,
        Err(error) => panic!("测试端口范围创建失败: {error}"),
    }
}
