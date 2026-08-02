use std::net::IpAddr;

use nonproxy_model::{
    AppMatcher, DecisionSpec, DomainMatchKind, FailureMode, IpFamily, NetworkFingerprintKind,
    NetworkProfileBinding, Platform, Policy, PolicyMatch, PolicySourceKind, ProxyTarget,
    RouteAction, RuntimeOverrideMode, RuntimeRoutingOverride, Transport,
};
use sha2::{Digest, Sha256};

use crate::CompileCapabilities;

pub(crate) fn content_hash(
    schema_version: u32,
    default_decision: &DecisionSpec,
    policies: &[&Policy],
    capabilities: &CompileCapabilities,
    network_profiles: Option<&[NetworkProfileBinding]>,
    runtime_override: Option<Option<&RuntimeRoutingOverride>>,
) -> [u8; 32] {
    let mut bytes = Vec::new();
    write_u32(&mut bytes, schema_version);
    write_decision(&mut bytes, default_decision);
    write_u64(&mut bytes, policies.len() as u64);
    for policy in policies {
        write_string(&mut bytes, policy.id().as_str());
        bytes.push(source_code(policy.source_kind()));
        write_i32(&mut bytes, policy.priority());
        write_bytes(&mut bytes, &matcher_bytes(policy.matcher()));
        write_decision(&mut bytes, policy.decision());
    }
    write_u64(&mut bytes, capabilities.outbounds().len() as u64);
    for (outbound_id, capabilities) in capabilities.outbounds() {
        write_string(&mut bytes, outbound_id.as_str());
        bytes.push(u8::from(capabilities.supports_transport(Transport::Tcp)));
        bytes.push(u8::from(capabilities.supports_transport(Transport::Udp)));
        bytes.push(u8::from(capabilities.supports_family(IpFamily::Ipv4)));
        bytes.push(u8::from(capabilities.supports_family(IpFamily::Ipv6)));
    }
    if let Some(network_profiles) = network_profiles {
        let mut network_profiles = network_profiles.iter().collect::<Vec<_>>();
        network_profiles.sort_by(|left, right| left.id().cmp(right.id()));
        write_u64(&mut bytes, network_profiles.len() as u64);
        for profile in network_profiles {
            write_string(&mut bytes, profile.id().as_str());
            bytes.push(match profile.fingerprint().kind() {
                NetworkFingerprintKind::WifiSsidSha256 => 1,
                NetworkFingerprintKind::DefaultGatewaySha256 => 2,
                NetworkFingerprintKind::InterfaceClass => 3,
            });
            write_string(&mut bytes, profile.fingerprint().value());
        }
    }
    if let Some(runtime_override) = runtime_override {
        match runtime_override {
            Some(value) => {
                bytes.push(1);
                bytes.push(match value.mode() {
                    RuntimeOverrideMode::Paused => 1,
                    RuntimeOverrideMode::Direct => 2,
                    RuntimeOverrideMode::Proxy => 3,
                });
                write_optional_string(
                    &mut bytes,
                    value.outbound_id().map(|outbound| outbound.as_str()),
                );
                write_u64(&mut bytes, value.expires_at_unix_ms());
            }
            None => bytes.push(0),
        }
    }
    if !capabilities.outbound_groups().is_empty() {
        bytes.extend_from_slice(b"NP_GROUPS_V1");
        write_u64(&mut bytes, capabilities.outbound_groups().len() as u64);
        for (group_id, group) in capabilities.outbound_groups() {
            write_string(&mut bytes, group_id.as_str());
            write_u64(&mut bytes, group.revision());
            write_u64(&mut bytes, group.members().len() as u64);
            for member in group.members() {
                write_string(&mut bytes, member.as_str());
            }
            match capabilities
                .outbound_group_capabilities()
                .get(group_id)
                .copied()
            {
                Some(capabilities) => {
                    bytes.push(u8::from(capabilities.supports_transport(Transport::Tcp)));
                    bytes.push(u8::from(capabilities.supports_transport(Transport::Udp)));
                    bytes.push(u8::from(capabilities.supports_family(IpFamily::Ipv4)));
                    bytes.push(u8::from(capabilities.supports_family(IpFamily::Ipv6)));
                }
                None => bytes.extend_from_slice(&[u8::MAX; 4]),
            }
        }
    }
    Sha256::digest(bytes).into()
}

pub(crate) fn matcher_bytes(matcher: &PolicyMatch) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_optional_app(&mut bytes, matcher.app());
    match matcher.domain() {
        Some(domain) => {
            bytes.push(1);
            bytes.push(match domain.kind() {
                DomainMatchKind::Exact => 1,
                DomainMatchKind::Suffix => 2,
                DomainMatchKind::RegistrableDomain => 3,
            });
            write_string(&mut bytes, domain.pattern().as_ascii());
        }
        None => bytes.push(0),
    }
    match matcher.cidr() {
        Some(cidr) => {
            bytes.push(1);
            match cidr.network() {
                IpAddr::V4(address) => {
                    bytes.push(4);
                    bytes.extend_from_slice(&address.octets());
                }
                IpAddr::V6(address) => {
                    bytes.push(6);
                    bytes.extend_from_slice(&address.octets());
                }
            }
            bytes.push(cidr.prefix_length());
        }
        None => bytes.push(0),
    }
    match matcher.network() {
        Some(network) => {
            bytes.push(1);
            write_string(&mut bytes, network.profile_id().as_str());
        }
        None => bytes.push(0),
    }
    write_u64(&mut bytes, matcher.transports().len() as u64);
    for transport in matcher.transports() {
        bytes.push(transport_code(*transport));
    }
    write_u64(&mut bytes, matcher.ports().len() as u64);
    for port in matcher.ports() {
        write_u16(&mut bytes, port.first());
        write_u16(&mut bytes, port.last());
    }
    bytes
}

fn write_optional_app(bytes: &mut Vec<u8>, matcher: Option<&AppMatcher>) {
    match matcher {
        Some(app) => {
            bytes.push(1);
            bytes.push(match app.platform() {
                Platform::MacOs => 1,
                Platform::Windows => 2,
            });
            write_string(bytes, app.stable_id());
            write_optional_string(bytes, app.signer_id());
            bytes.push(u8::from(app.includes_helpers()));
        }
        None => bytes.push(0),
    }
}

fn write_decision(bytes: &mut Vec<u8>, decision: &DecisionSpec) {
    bytes.push(match decision.action() {
        RouteAction::Direct => 1,
        RouteAction::Proxy => 2,
        RouteAction::Block => 3,
    });
    match decision.proxy_target() {
        None => bytes.push(0),
        Some(ProxyTarget::Outbound(value)) => {
            bytes.push(1);
            write_string(bytes, value.as_str());
        }
        Some(ProxyTarget::Group(value)) => {
            bytes.push(2);
            write_string(bytes, value.as_str());
        }
    }
    bytes.push(match decision.failure_mode() {
        FailureMode::Closed => 1,
        FailureMode::Open => 2,
    });
}

fn source_code(source: PolicySourceKind) -> u8 {
    match source {
        PolicySourceKind::System => 1,
        PolicySourceKind::AppDestination => 2,
        PolicySourceKind::App => 3,
        PolicySourceKind::Site => 4,
        PolicySourceKind::Network => 5,
        PolicySourceKind::BuiltIn => 6,
        PolicySourceKind::Cidr => 7,
        PolicySourceKind::Adapter => 8,
    }
}

fn transport_code(transport: Transport) -> u8 {
    match transport {
        Transport::Tcp => 1,
        Transport::Udp => 2,
    }
}

fn write_optional_string(bytes: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            bytes.push(1);
            write_string(bytes, value);
        }
        None => bytes.push(0),
    }
}

fn write_string(bytes: &mut Vec<u8>, value: &str) {
    write_bytes(bytes, value.as_bytes());
}

fn write_bytes(bytes: &mut Vec<u8>, value: &[u8]) {
    write_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value);
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
