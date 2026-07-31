use nonproxy_model::{IpFamily, PolicySourceKind, RouteAction, Transport};
use nonproxy_proto::{
    common::v1::{ComponentKind, Severity},
    events::v1::RuntimeState,
};
use nonproxy_storage::{EvidenceLevel, OutboundKind};

use crate::Gateway;

pub(crate) fn capabilities(gateway: &Gateway) -> Vec<&'static str> {
    let value = gateway.capabilities();
    let mut result = Vec::new();
    if value.supports_app_matching() {
        result.push("app_match");
    }
    if value.supports_domain_matching() {
        result.push("domain_match");
    }
    if value.supports_cidr_matching() {
        result.push("cidr_match");
    }
    if value.supports_transport(Transport::Tcp) {
        result.push("tcp");
    }
    if value.supports_transport(Transport::Udp) {
        result.push("udp");
    }
    if value.supports_family(IpFamily::Ipv4) {
        result.push("ipv4");
    }
    if value.supports_family(IpFamily::Ipv6) {
        result.push("ipv6");
    }
    result
}

pub(crate) const fn policy_source(value: PolicySourceKind) -> &'static str {
    match value {
        PolicySourceKind::System => "system",
        PolicySourceKind::AppDestination => "app_destination",
        PolicySourceKind::App => "app",
        PolicySourceKind::Site => "site",
        PolicySourceKind::Network => "network",
        PolicySourceKind::BuiltIn => "built_in",
        PolicySourceKind::Cidr => "cidr",
        PolicySourceKind::Adapter => "adapter",
    }
}

pub(crate) const fn action(value: RouteAction) -> &'static str {
    match value {
        RouteAction::Direct => "direct",
        RouteAction::Proxy => "proxy",
        RouteAction::Block => "block",
    }
}

pub(crate) const fn transport(value: Transport) -> &'static str {
    match value {
        Transport::Tcp => "tcp",
        Transport::Udp => "udp",
    }
}

pub(crate) const fn evidence(value: EvidenceLevel) -> &'static str {
    match value {
        EvidenceLevel::Decision => "decision",
        EvidenceLevel::Path => "path",
        EvidenceLevel::Exit => "exit",
    }
}

pub(crate) const fn outbound_kind(value: OutboundKind) -> &'static str {
    match value {
        OutboundKind::HttpConnect => "http_connect",
        OutboundKind::Socks5 => "socks5",
        OutboundKind::Adapter => "adapter",
    }
}

pub(crate) fn severity(value: i32) -> &'static str {
    match Severity::try_from(value) {
        Ok(Severity::Debug) => "debug",
        Ok(Severity::Info) => "info",
        Ok(Severity::Warning) => "warning",
        Ok(Severity::Error) => "error",
        Ok(Severity::Critical) => "critical",
        _ => "unspecified",
    }
}

pub(crate) fn component(value: i32) -> &'static str {
    match ComponentKind::try_from(value) {
        Ok(ComponentKind::Desktop) => "desktop",
        Ok(ComponentKind::Gateway) => "gateway",
        Ok(ComponentKind::TransparentProxy) => "transparent_proxy",
        Ok(ComponentKind::DnsProxy) => "dns_proxy",
        Ok(ComponentKind::NativeMessagingHost) => "native_messaging_host",
        Ok(ComponentKind::AdapterHost) => "adapter_host",
        Ok(ComponentKind::WindowsService) => "windows_service",
        _ => "unspecified",
    }
}

pub(crate) fn runtime_state(value: i32) -> &'static str {
    match RuntimeState::try_from(value) {
        Ok(RuntimeState::Stopped) => "stopped",
        Ok(RuntimeState::Starting) => "starting",
        Ok(RuntimeState::Ready) => "ready",
        Ok(RuntimeState::Degraded) => "degraded",
        Ok(RuntimeState::Draining) => "draining",
        Ok(RuntimeState::Failed) => "failed",
        _ => "unspecified",
    }
}
