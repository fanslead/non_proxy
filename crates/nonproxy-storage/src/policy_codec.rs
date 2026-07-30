use std::str::FromStr;

use nonproxy_model::{
    AppMatcher, Cidr, DecisionSpec, DomainMatchKind, DomainMatcher, FailureMode, NetworkMatcher,
    NetworkProfileId, OutboundId, Platform, Policy, PolicyId, PolicyMatch, PolicyMetadata,
    PolicyOrigin, PolicySourceKind, PortRange, RouteAction, Transport,
};

use crate::StorageError;

pub(crate) struct RawPolicy {
    pub id: String,
    pub display_name: String,
    pub source_kind: i64,
    pub decision_action: i64,
    pub outbound_id: Option<String>,
    pub failure_mode: i64,
    pub priority: i64,
    pub enabled: i64,
    pub origin: i64,
    pub revision: i64,
    pub app_platform: Option<i64>,
    pub app_stable_id: Option<String>,
    pub app_signer_id: Option<String>,
    pub app_include_helpers: Option<i64>,
    pub cidr: Option<String>,
    pub network_profile_id: Option<String>,
}

impl RawPolicy {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            display_name: row.get(1)?,
            source_kind: row.get(2)?,
            decision_action: row.get(3)?,
            outbound_id: row.get(4)?,
            failure_mode: row.get(5)?,
            priority: row.get(6)?,
            enabled: row.get(7)?,
            origin: row.get(8)?,
            revision: row.get(9)?,
            app_platform: row.get(10)?,
            app_stable_id: row.get(11)?,
            app_signer_id: row.get(12)?,
            app_include_helpers: row.get(13)?,
            cidr: row.get(14)?,
            network_profile_id: row.get(15)?,
        })
    }
}

pub(crate) fn decode_policy(
    raw: RawPolicy,
    domain: Option<(i64, String)>,
    transport_values: Vec<i64>,
    port_values: Vec<(i64, i64)>,
) -> Result<Policy, StorageError> {
    let app = decode_app(&raw)?;
    let domain = domain
        .map(|(kind, pattern)| {
            DomainMatcher::new(decode_domain_kind(kind)?, &pattern).map_err(StorageError::from)
        })
        .transpose()?;
    let cidr = raw.cidr.as_deref().map(Cidr::from_str).transpose()?;
    let network = raw
        .network_profile_id
        .map(NetworkProfileId::new)
        .transpose()?
        .map(NetworkMatcher::new);
    let transports = transport_values
        .into_iter()
        .map(decode_transport)
        .collect::<Result<Vec<_>, _>>()?;
    let ports = port_values
        .into_iter()
        .map(|(first, last)| {
            let first = u16::try_from(first).map_err(|_| StorageError::CorruptData {
                field: "policy_port_range.first_port",
            })?;
            let last = u16::try_from(last).map_err(|_| StorageError::CorruptData {
                field: "policy_port_range.last_port",
            })?;
            PortRange::new(first, last).map_err(StorageError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let matcher = PolicyMatch::new(app, domain, cidr, network, transports, ports)?;
    let outbound_id = raw.outbound_id.map(OutboundId::new).transpose()?;
    let decision = DecisionSpec::new(
        decode_action(raw.decision_action)?,
        outbound_id,
        decode_failure_mode(raw.failure_mode)?,
    )?;
    let id = PolicyId::new(raw.id)?;
    let priority = i32::try_from(raw.priority).map_err(|_| StorageError::CorruptData {
        field: "policy.priority",
    })?;
    let revision = u64::try_from(raw.revision).map_err(|_| StorageError::CorruptData {
        field: "policy.revision",
    })?;
    let mut policy = Policy::new(
        id,
        raw.display_name,
        matcher,
        decision,
        PolicyMetadata::new(
            decode_source(raw.source_kind)?,
            priority,
            decode_origin(raw.origin)?,
            revision,
        ),
    )?;
    match raw.enabled {
        0 => policy = policy.disabled(),
        1 => {}
        _ => {
            return Err(StorageError::CorruptData {
                field: "policy.enabled",
            });
        }
    }
    Ok(policy)
}

fn decode_app(raw: &RawPolicy) -> Result<Option<AppMatcher>, StorageError> {
    match (
        raw.app_platform,
        raw.app_stable_id.as_deref(),
        raw.app_include_helpers,
    ) {
        (None, None, None) => Ok(None),
        (Some(platform), Some(stable_id), Some(include_helpers)) => {
            let mut matcher = AppMatcher::new(decode_platform(platform)?, stable_id)?;
            if let Some(signer_id) = raw.app_signer_id.as_deref() {
                matcher = matcher.with_signer_id(signer_id)?;
            }
            match include_helpers {
                0 => {}
                1 => matcher = matcher.include_helpers(true),
                _ => {
                    return Err(StorageError::CorruptData {
                        field: "policy.app_include_helpers",
                    });
                }
            }
            Ok(Some(matcher))
        }
        _ => Err(StorageError::CorruptData {
            field: "policy.app_matcher",
        }),
    }
}

pub(crate) const fn source_code(value: PolicySourceKind) -> i64 {
    match value {
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

fn decode_source(value: i64) -> Result<PolicySourceKind, StorageError> {
    match value {
        1 => Ok(PolicySourceKind::System),
        2 => Ok(PolicySourceKind::AppDestination),
        3 => Ok(PolicySourceKind::App),
        4 => Ok(PolicySourceKind::Site),
        5 => Ok(PolicySourceKind::Network),
        6 => Ok(PolicySourceKind::BuiltIn),
        7 => Ok(PolicySourceKind::Cidr),
        8 => Ok(PolicySourceKind::Adapter),
        _ => Err(StorageError::CorruptData {
            field: "policy.source_kind",
        }),
    }
}

pub(crate) const fn origin_code(value: PolicyOrigin) -> i64 {
    match value {
        PolicyOrigin::System => 1,
        PolicyOrigin::User => 2,
        PolicyOrigin::SignedBuiltIn => 3,
        PolicyOrigin::Subscription => 4,
        PolicyOrigin::Adapter => 5,
    }
}

fn decode_origin(value: i64) -> Result<PolicyOrigin, StorageError> {
    match value {
        1 => Ok(PolicyOrigin::System),
        2 => Ok(PolicyOrigin::User),
        3 => Ok(PolicyOrigin::SignedBuiltIn),
        4 => Ok(PolicyOrigin::Subscription),
        5 => Ok(PolicyOrigin::Adapter),
        _ => Err(StorageError::CorruptData {
            field: "policy.origin",
        }),
    }
}

pub(crate) const fn platform_code(value: Platform) -> i64 {
    match value {
        Platform::MacOs => 1,
        Platform::Windows => 2,
    }
}

fn decode_platform(value: i64) -> Result<Platform, StorageError> {
    match value {
        1 => Ok(Platform::MacOs),
        2 => Ok(Platform::Windows),
        _ => Err(StorageError::CorruptData {
            field: "policy.app_platform",
        }),
    }
}

pub(crate) const fn domain_kind_code(value: DomainMatchKind) -> i64 {
    match value {
        DomainMatchKind::Exact => 1,
        DomainMatchKind::Suffix => 2,
        DomainMatchKind::RegistrableDomain => 3,
    }
}

fn decode_domain_kind(value: i64) -> Result<DomainMatchKind, StorageError> {
    match value {
        1 => Ok(DomainMatchKind::Exact),
        2 => Ok(DomainMatchKind::Suffix),
        3 => Ok(DomainMatchKind::RegistrableDomain),
        _ => Err(StorageError::CorruptData {
            field: "domain_target.match_kind",
        }),
    }
}

pub(crate) const fn action_code(value: RouteAction) -> i64 {
    match value {
        RouteAction::Direct => 1,
        RouteAction::Proxy => 2,
        RouteAction::Block => 3,
    }
}

fn decode_action(value: i64) -> Result<RouteAction, StorageError> {
    match value {
        1 => Ok(RouteAction::Direct),
        2 => Ok(RouteAction::Proxy),
        3 => Ok(RouteAction::Block),
        _ => Err(StorageError::CorruptData {
            field: "policy.decision_action",
        }),
    }
}

pub(crate) const fn failure_mode_code(value: FailureMode) -> i64 {
    match value {
        FailureMode::Closed => 1,
        FailureMode::Open => 2,
    }
}

fn decode_failure_mode(value: i64) -> Result<FailureMode, StorageError> {
    match value {
        1 => Ok(FailureMode::Closed),
        2 => Ok(FailureMode::Open),
        _ => Err(StorageError::CorruptData {
            field: "policy.failure_mode",
        }),
    }
}

pub(crate) const fn transport_code(value: Transport) -> i64 {
    match value {
        Transport::Tcp => 1,
        Transport::Udp => 2,
    }
}

fn decode_transport(value: i64) -> Result<Transport, StorageError> {
    match value {
        1 => Ok(Transport::Tcp),
        2 => Ok(Transport::Udp),
        _ => Err(StorageError::CorruptData {
            field: "policy_transport.transport",
        }),
    }
}
