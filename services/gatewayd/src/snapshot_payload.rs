use std::collections::HashSet;

use nonproxy_model::{
    DecisionSpec, IpFamily, NetworkFingerprint, NetworkFingerprintKind, NetworkProfileBinding,
    NetworkProfileId, OutboundId, Policy, Transport,
};
use nonproxy_policy::OutboundCapabilities;
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest};
use nonproxy_proto::{
    common::v1 as common_proto,
    policy::v1::{
        CompileCapabilitySet, CompiledPolicyPayload,
        NetworkFingerprintKind as ProtoFingerprintKind,
        NetworkProfileBinding as ProtoProfileBinding, OutboundCapabilitySpec,
    },
};
use prost::Message;

use crate::{
    GatewayError,
    proto_policy::{policy_from_proto, policy_to_proto},
};

pub const SNAPSHOT_PAYLOAD_FORMAT: &str = "nonproxy.compiled-policy.v1";
const LEGACY_SNAPSHOT_PAYLOAD_VERSION: u32 = 1;
const SNAPSHOT_PAYLOAD_VERSION: u32 = 2;

pub(crate) struct DecodedSnapshotPayload {
    pub policies: Vec<Policy>,
    pub capabilities: CompileCapabilities,
    pub default_decision: DecisionSpec,
    pub network_profiles: Vec<NetworkProfileBinding>,
    pub includes_network_profiles: bool,
}

impl DecodedSnapshotPayload {
    pub fn into_compile_request(
        self,
        snapshot_version: u64,
        created_at_unix_ms: u64,
    ) -> CompileRequest {
        let includes_network_profiles = self.includes_network_profiles;
        let network_profiles = self.network_profiles;
        let request = CompileRequest::new(
            snapshot_version,
            created_at_unix_ms,
            self.default_decision,
            self.policies,
            self.capabilities,
        );
        if includes_network_profiles {
            request.with_network_profiles(network_profiles)
        } else {
            request
        }
    }
}

pub fn encode(
    policies: &[Policy],
    capabilities: &CompileCapabilities,
    default_decision: &DecisionSpec,
    network_profiles: &[NetworkProfileBinding],
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
        network_profiles: sorted_profiles_to_proto(network_profiles),
    };
    let mut bytes = Vec::with_capacity(payload.encoded_len());
    payload.encode(&mut bytes)?;
    Ok(bytes)
}

pub fn decode(
    bytes: &[u8],
) -> Result<(Vec<Policy>, CompileCapabilities, DecisionSpec), GatewayError> {
    let decoded = decode_versioned(bytes)?;
    Ok((
        decoded.policies,
        decoded.capabilities,
        decoded.default_decision,
    ))
}

pub(crate) fn decode_versioned(bytes: &[u8]) -> Result<DecodedSnapshotPayload, GatewayError> {
    let payload = CompiledPolicyPayload::decode(bytes)?;
    if payload.format_version != LEGACY_SNAPSHOT_PAYLOAD_VERSION
        && payload.format_version != SNAPSHOT_PAYLOAD_VERSION
    {
        return Err(GatewayError::InvalidContract("快照载荷版本不受支持"));
    }
    let includes_network_profiles = payload.format_version == SNAPSHOT_PAYLOAD_VERSION;
    if !includes_network_profiles && !payload.network_profiles.is_empty() {
        return Err(GatewayError::InvalidContract(
            "旧版快照不能包含网络配置档目录",
        ));
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
    let network_profiles = if includes_network_profiles {
        profiles_from_proto(payload.network_profiles)?
    } else {
        Vec::new()
    };
    Ok(DecodedSnapshotPayload {
        policies,
        capabilities,
        default_decision,
        network_profiles,
        includes_network_profiles,
    })
}

fn sorted_profiles_to_proto(values: &[NetworkProfileBinding]) -> Vec<ProtoProfileBinding> {
    let mut values = values.iter().collect::<Vec<_>>();
    values.sort_by(|left, right| left.id().cmp(right.id()));
    values.into_iter().map(profile_to_proto).collect()
}

fn profile_to_proto(value: &NetworkProfileBinding) -> ProtoProfileBinding {
    ProtoProfileBinding {
        id: value.id().as_str().to_owned(),
        fingerprint_kind: match value.fingerprint().kind() {
            NetworkFingerprintKind::WifiSsidSha256 => ProtoFingerprintKind::WifiSsidSha256,
            NetworkFingerprintKind::DefaultGatewaySha256 => {
                ProtoFingerprintKind::DefaultGatewaySha256
            }
            NetworkFingerprintKind::InterfaceClass => ProtoFingerprintKind::InterfaceClass,
        } as i32,
        fingerprint_value: value.fingerprint().value().to_owned(),
    }
}

fn profiles_from_proto(
    values: Vec<ProtoProfileBinding>,
) -> Result<Vec<NetworkProfileBinding>, GatewayError> {
    let mut profiles = Vec::with_capacity(values.len());
    let mut ids = HashSet::new();
    let mut fingerprints = HashSet::new();
    let mut previous_id: Option<String> = None;
    for value in values {
        if previous_id.as_ref().is_some_and(|id| id >= &value.id) {
            return Err(GatewayError::InvalidContract(
                "网络配置档目录未按稳定标识排序",
            ));
        }
        let kind = match ProtoFingerprintKind::try_from(value.fingerprint_kind) {
            Ok(ProtoFingerprintKind::WifiSsidSha256) => NetworkFingerprintKind::WifiSsidSha256,
            Ok(ProtoFingerprintKind::DefaultGatewaySha256) => {
                NetworkFingerprintKind::DefaultGatewaySha256
            }
            Ok(ProtoFingerprintKind::InterfaceClass) => NetworkFingerprintKind::InterfaceClass,
            Ok(ProtoFingerprintKind::Unspecified) | Err(_) => {
                return Err(GatewayError::InvalidContract("网络配置档指纹类型无效"));
            }
        };
        let profile = NetworkProfileBinding::new(
            NetworkProfileId::new(value.id.clone())?,
            NetworkFingerprint::new(kind, value.fingerprint_value)?,
        );
        if !ids.insert(profile.id().clone()) || !fingerprints.insert(profile.fingerprint().clone())
        {
            return Err(GatewayError::InvalidContract(
                "网络配置档目录包含重复标识或指纹",
            ));
        }
        previous_id = Some(value.id);
        profiles.push(profile);
    }
    Ok(profiles)
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
    use nonproxy_model::{
        DecisionSpec, NetworkFingerprint, NetworkFingerprintKind, NetworkProfileBinding,
        NetworkProfileId,
    };
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_proto::{
        common::v1::{IpFamily, TransportProtocol},
        policy::v1::{CompileCapabilitySet, CompiledPolicyPayload, OutboundCapabilitySpec},
    };
    use prost::Message;

    use super::{
        LEGACY_SNAPSHOT_PAYLOAD_VERSION, SNAPSHOT_PAYLOAD_VERSION, decode, decode_versioned, encode,
    };

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

    #[test]
    fn version_two_round_trip_preserves_privacy_safe_network_catalog() {
        let profile = NetworkProfileBinding::new(
            NetworkProfileId::new("office")
                .unwrap_or_else(|error| panic!("测试网络标识创建失败: {error}")),
            NetworkFingerprint::new(NetworkFingerprintKind::WifiSsidSha256, "a".repeat(64))
                .unwrap_or_else(|error| panic!("测试网络指纹创建失败: {error}")),
        );
        let bytes = encode(
            &[],
            &CompileCapabilities::full(),
            &DecisionSpec::direct(),
            std::slice::from_ref(&profile),
        )
        .unwrap_or_else(|error| panic!("网络配置档快照编码失败: {error}"));

        let decoded = decode_versioned(&bytes)
            .unwrap_or_else(|error| panic!("网络配置档快照解码失败: {error}"));

        assert!(decoded.includes_network_profiles);
        assert_eq!(decoded.network_profiles, vec![profile]);
    }

    #[test]
    fn legacy_version_without_catalog_remains_readable() {
        let mut payload = payload(Vec::new(), Vec::new());
        payload.format_version = LEGACY_SNAPSHOT_PAYLOAD_VERSION;

        let decoded = decode_versioned(&payload.encode_to_vec())
            .unwrap_or_else(|error| panic!("旧版快照解码失败: {error}"));

        assert!(!decoded.includes_network_profiles);
        assert!(decoded.network_profiles.is_empty());
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
            network_profiles: Vec::new(),
        }
    }
}
