use std::collections::HashSet;

use nonproxy_model::{DecisionSpec, IpFamily, OutboundId, Policy, Transport};
use nonproxy_policy::OutboundCapabilities;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1 as common_proto,
    policy::v1::{CompileCapabilitySet, CompiledPolicyPayload, OutboundCapabilitySpec},
};
use prost::Message;

use crate::{
    GatewayError,
    proto_policy::{policy_from_proto, policy_to_proto},
};

pub const SNAPSHOT_PAYLOAD_FORMAT: &str = "nonproxy.compiled-policy.v1";
const SNAPSHOT_PAYLOAD_VERSION: u32 = 1;

pub fn encode(
    policies: &[Policy],
    capabilities: &CompileCapabilities,
    default_decision: &DecisionSpec,
) -> Result<Vec<u8>, GatewayError> {
    let mut enabled = policies
        .iter()
        .filter(|policy| policy.enabled())
        .collect::<Vec<_>>();
    enabled.sort_by(|left, right| left.id().cmp(right.id()));
    let payload = CompiledPolicyPayload {
        format_version: SNAPSHOT_PAYLOAD_VERSION,
        policies: enabled.into_iter().map(policy_to_proto).collect(),
        capabilities: Some(capabilities_to_proto(capabilities)),
        default_decision: Some(crate::proto_policy::decision_to_proto(default_decision)),
    };
    let mut bytes = Vec::with_capacity(payload.encoded_len());
    payload.encode(&mut bytes)?;
    Ok(bytes)
}

pub fn decode(
    bytes: &[u8],
) -> Result<(Vec<Policy>, CompileCapabilities, DecisionSpec), GatewayError> {
    let payload = CompiledPolicyPayload::decode(bytes)?;
    if payload.format_version != SNAPSHOT_PAYLOAD_VERSION {
        return Err(GatewayError::InvalidContract("快照载荷版本不受支持"));
    }
    let policies = payload
        .policies
        .into_iter()
        .map(policy_from_proto)
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_policy_ids(&policies)?;
    let capabilities = capabilities_from_proto(
        payload
            .capabilities
            .ok_or(GatewayError::InvalidContract("快照缺少能力集合"))?,
    )?;
    let default_decision = crate::proto_policy::decision_from_proto(
        payload
            .default_decision
            .ok_or(GatewayError::InvalidContract("快照缺少默认决策"))?,
    )?;
    Ok((policies, capabilities, default_decision))
}

fn capabilities_to_proto(value: &CompileCapabilities) -> CompileCapabilitySet {
    let transports = [Transport::Tcp, Transport::Udp]
        .into_iter()
        .filter(|item| value.supports_transport(*item))
        .map(|item| match item {
            Transport::Tcp => common_proto::TransportProtocol::Tcp as i32,
            Transport::Udp => common_proto::TransportProtocol::Udp as i32,
        })
        .collect();
    let ip_families = [IpFamily::Ipv4, IpFamily::Ipv6]
        .into_iter()
        .filter(|item| value.supports_family(*item))
        .map(|item| match item {
            IpFamily::Ipv4 => common_proto::IpFamily::Ipv4 as i32,
            IpFamily::Ipv6 => common_proto::IpFamily::Ipv6 as i32,
        })
        .collect();
    let outbounds = value
        .outbounds()
        .iter()
        .map(|(id, capabilities)| outbound_to_proto(id, *capabilities))
        .collect();
    CompileCapabilitySet {
        app_match: value.supports_app_matching(),
        domain_match: value.supports_domain_matching(),
        cidr_match: value.supports_cidr_matching(),
        transports,
        ip_families,
        outbounds,
    }
}

fn capabilities_from_proto(
    value: CompileCapabilitySet,
) -> Result<CompileCapabilities, GatewayError> {
    let target = OutboundCapabilities::new(
        has_transport(&value.transports, common_proto::TransportProtocol::Tcp)?,
        has_transport(&value.transports, common_proto::TransportProtocol::Udp)?,
        has_family(&value.ip_families, common_proto::IpFamily::Ipv4)?,
        has_family(&value.ip_families, common_proto::IpFamily::Ipv6)?,
    );
    let mut capabilities = CompileCapabilities::new(
        value.app_match,
        value.domain_match,
        value.cidr_match,
        target,
    );
    let mut outbound_ids = HashSet::new();
    for outbound in value.outbounds {
        if !outbound_ids.insert(outbound.outbound_id.clone()) {
            return Err(GatewayError::InvalidContract("快照包含重复出口能力"));
        }
        let id = OutboundId::new(outbound.outbound_id)?;
        let outbound_capabilities = OutboundCapabilities::new(
            has_transport(&outbound.transports, common_proto::TransportProtocol::Tcp)?,
            has_transport(&outbound.transports, common_proto::TransportProtocol::Udp)?,
            has_family(&outbound.ip_families, common_proto::IpFamily::Ipv4)?,
            has_family(&outbound.ip_families, common_proto::IpFamily::Ipv6)?,
        );
        capabilities = capabilities.with_outbound(id, outbound_capabilities);
    }
    Ok(capabilities)
}

fn outbound_to_proto(
    id: &OutboundId,
    capabilities: OutboundCapabilities,
) -> OutboundCapabilitySpec {
    let transports = [Transport::Tcp, Transport::Udp]
        .into_iter()
        .filter(|item| capabilities.supports_transport(*item))
        .map(|item| match item {
            Transport::Tcp => common_proto::TransportProtocol::Tcp as i32,
            Transport::Udp => common_proto::TransportProtocol::Udp as i32,
        })
        .collect();
    let ip_families = [IpFamily::Ipv4, IpFamily::Ipv6]
        .into_iter()
        .filter(|item| capabilities.supports_family(*item))
        .map(|item| match item {
            IpFamily::Ipv4 => common_proto::IpFamily::Ipv4 as i32,
            IpFamily::Ipv6 => common_proto::IpFamily::Ipv6 as i32,
        })
        .collect();
    OutboundCapabilitySpec {
        outbound_id: id.as_str().to_owned(),
        transports,
        ip_families,
    }
}

fn has_transport(
    values: &[i32],
    target: common_proto::TransportProtocol,
) -> Result<bool, GatewayError> {
    validate_enum_values(
        values,
        common_proto::TransportProtocol::Unspecified as i32,
        |value| common_proto::TransportProtocol::try_from(value).is_ok(),
    )?;
    Ok(values.contains(&(target as i32)))
}

fn has_family(values: &[i32], target: common_proto::IpFamily) -> Result<bool, GatewayError> {
    validate_enum_values(
        values,
        common_proto::IpFamily::Unspecified as i32,
        |value| common_proto::IpFamily::try_from(value).is_ok(),
    )?;
    Ok(values.contains(&(target as i32)))
}

fn validate_enum_values(
    values: &[i32],
    unspecified: i32,
    is_known: impl Fn(i32) -> bool,
) -> Result<(), GatewayError> {
    let mut seen = HashSet::new();
    for value in values.iter().copied() {
        if value == unspecified || !is_known(value) {
            return Err(GatewayError::InvalidContract("快照包含无效能力枚举值"));
        }
        if !seen.insert(value) {
            return Err(GatewayError::InvalidContract("快照包含重复能力枚举值"));
        }
    }
    Ok(())
}

fn validate_unique_policy_ids(policies: &[Policy]) -> Result<(), GatewayError> {
    let mut ids = HashSet::new();
    if policies
        .iter()
        .any(|policy| !ids.insert(policy.id().as_str()))
    {
        return Err(GatewayError::InvalidContract("快照包含重复策略"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::{
        common::v1::{IpFamily, TransportProtocol},
        policy::v1::{CompileCapabilitySet, CompiledPolicyPayload, OutboundCapabilitySpec},
    };
    use prost::Message;

    use super::{SNAPSHOT_PAYLOAD_VERSION, decode};

    #[test]
    fn rejects_unspecified_capability_value() {
        let payload = payload(vec![TransportProtocol::Unspecified as i32], Vec::new());

        let result = decode(&payload.encode_to_vec());

        assert!(result.is_err());
    }

    #[test]
    fn rejects_duplicate_capability_value() {
        let value = TransportProtocol::Tcp as i32;
        let payload = payload(vec![value, value], Vec::new());

        let result = decode(&payload.encode_to_vec());

        assert!(result.is_err());
    }

    #[test]
    fn rejects_duplicate_outbound_capability() {
        let outbound = OutboundCapabilitySpec {
            outbound_id: "corp-proxy".to_owned(),
            transports: vec![TransportProtocol::Tcp as i32],
            ip_families: vec![IpFamily::Ipv4 as i32],
        };
        let payload = payload(Vec::new(), vec![outbound.clone(), outbound]);

        let result = decode(&payload.encode_to_vec());

        assert!(result.is_err());
    }

    fn payload(
        transports: Vec<i32>,
        outbounds: Vec<OutboundCapabilitySpec>,
    ) -> CompiledPolicyPayload {
        CompiledPolicyPayload {
            format_version: SNAPSHOT_PAYLOAD_VERSION,
            policies: Vec::new(),
            capabilities: Some(CompileCapabilitySet {
                app_match: true,
                domain_match: true,
                cidr_match: true,
                transports,
                ip_families: vec![IpFamily::Ipv4 as i32],
                outbounds,
            }),
            default_decision: Some(nonproxy_proto::policy::v1::DecisionSpec {
                action: nonproxy_proto::common::v1::RouteAction::Direct as i32,
                outbound_id: String::new(),
                failure_mode: nonproxy_proto::common::v1::FailureMode::Closed as i32,
            }),
        }
    }
}
