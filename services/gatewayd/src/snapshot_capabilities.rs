use std::collections::HashSet;

use nonproxy_model::{IpFamily, OutboundGroupId, OutboundGroupSpec, OutboundId, Transport};
use nonproxy_policy::OutboundCapabilities;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1 as common_proto,
    policy::v1::{CompileCapabilitySet, OutboundCapabilitySpec, OutboundGroupCapabilitySpec},
};

use crate::GatewayError;

pub(crate) fn to_proto(value: &CompileCapabilities) -> Result<CompileCapabilitySet, GatewayError> {
    let transports = [Transport::Tcp, Transport::Udp]
        .into_iter()
        .filter(|item| value.supports_transport(*item))
        .map(transport_to_proto)
        .collect();
    let ip_families = [IpFamily::Ipv4, IpFamily::Ipv6]
        .into_iter()
        .filter(|item| value.supports_family(*item))
        .map(family_to_proto)
        .collect();
    let outbounds = value
        .outbounds()
        .iter()
        .map(|(id, capabilities)| outbound_to_proto(id, *capabilities))
        .collect();
    let outbound_groups = value
        .outbound_groups()
        .iter()
        .map(|(id, group)| {
            value
                .outbound_group_capabilities()
                .get(id)
                .copied()
                .ok_or(GatewayError::InvalidContract("出口组目录缺少能力交集"))
                .map(|capabilities| group_to_proto(group, capabilities))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CompileCapabilitySet {
        app_match: value.supports_app_matching(),
        domain_match: value.supports_domain_matching(),
        cidr_match: value.supports_cidr_matching(),
        transports,
        ip_families,
        outbounds,
        outbound_groups,
    })
}

pub(crate) fn from_proto(value: CompileCapabilitySet) -> Result<CompileCapabilities, GatewayError> {
    let target = capabilities_from_values(&value.transports, &value.ip_families)?;
    let mut capabilities = CompileCapabilities::new(
        value.app_match,
        value.domain_match,
        value.cidr_match,
        target,
    );
    let mut previous_outbound_id: Option<String> = None;
    for outbound in value.outbounds {
        require_sorted_id(
            previous_outbound_id.as_deref(),
            &outbound.outbound_id,
            "快照出口能力目录未按稳定标识排序",
        )?;
        let id = OutboundId::new(outbound.outbound_id.clone())?;
        let outbound_capabilities =
            capabilities_from_values(&outbound.transports, &outbound.ip_families)?;
        previous_outbound_id = Some(outbound.outbound_id);
        capabilities = capabilities.with_outbound(id, outbound_capabilities);
    }
    let mut previous_group_id: Option<String> = None;
    for group in value.outbound_groups {
        require_sorted_id(
            previous_group_id.as_deref(),
            &group.outbound_group_id,
            "快照出口组目录未按稳定标识排序",
        )?;
        let id = OutboundGroupId::new(group.outbound_group_id.clone())?;
        let members = group
            .outbound_ids
            .into_iter()
            .map(OutboundId::new)
            .collect::<Result<Vec<_>, _>>()?;
        let spec = OutboundGroupSpec::new(id.clone(), group.revision, members)?;
        let encoded_capabilities = capabilities_from_values(&group.transports, &group.ip_families)?;
        capabilities = capabilities.with_outbound_group(spec)?;
        if capabilities.outbound_group_capabilities().get(&id) != Some(&encoded_capabilities) {
            return Err(GatewayError::InvalidContract(
                "出口组能力交集与成员目录不一致",
            ));
        }
        previous_group_id = Some(group.outbound_group_id);
    }
    Ok(capabilities)
}

fn require_sorted_id(
    previous: Option<&str>,
    current: &str,
    message: &'static str,
) -> Result<(), GatewayError> {
    if previous.is_some_and(|value| value >= current) {
        return Err(GatewayError::InvalidContract(message));
    }
    Ok(())
}

fn group_to_proto(
    group: &OutboundGroupSpec,
    capabilities: OutboundCapabilities,
) -> OutboundGroupCapabilitySpec {
    let (transports, ip_families) = capability_values(capabilities);
    OutboundGroupCapabilitySpec {
        outbound_group_id: group.id().as_str().to_owned(),
        revision: group.revision(),
        outbound_ids: group
            .members()
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect(),
        transports,
        ip_families,
    }
}

fn outbound_to_proto(
    id: &OutboundId,
    capabilities: OutboundCapabilities,
) -> OutboundCapabilitySpec {
    let (transports, ip_families) = capability_values(capabilities);
    OutboundCapabilitySpec {
        outbound_id: id.as_str().to_owned(),
        transports,
        ip_families,
    }
}

fn capability_values(capabilities: OutboundCapabilities) -> (Vec<i32>, Vec<i32>) {
    let transports = [Transport::Tcp, Transport::Udp]
        .into_iter()
        .filter(|item| capabilities.supports_transport(*item))
        .map(transport_to_proto)
        .collect();
    let ip_families = [IpFamily::Ipv4, IpFamily::Ipv6]
        .into_iter()
        .filter(|item| capabilities.supports_family(*item))
        .map(family_to_proto)
        .collect();
    (transports, ip_families)
}

fn capabilities_from_values(
    transports: &[i32],
    ip_families: &[i32],
) -> Result<OutboundCapabilities, GatewayError> {
    Ok(OutboundCapabilities::new(
        has_transport(transports, common_proto::TransportProtocol::Tcp)?,
        has_transport(transports, common_proto::TransportProtocol::Udp)?,
        has_family(ip_families, common_proto::IpFamily::Ipv4)?,
        has_family(ip_families, common_proto::IpFamily::Ipv6)?,
    ))
}

fn transport_to_proto(value: Transport) -> i32 {
    match value {
        Transport::Tcp => common_proto::TransportProtocol::Tcp as i32,
        Transport::Udp => common_proto::TransportProtocol::Udp as i32,
    }
}

fn family_to_proto(value: IpFamily) -> i32 {
    match value {
        IpFamily::Ipv4 => common_proto::IpFamily::Ipv4 as i32,
        IpFamily::Ipv6 => common_proto::IpFamily::Ipv6 as i32,
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
