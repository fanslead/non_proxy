use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr, SocketAddrV6},
    num::NonZeroU32,
};

use nonproxy_dns::{DnsName, ParsedDnsQuery};
use nonproxy_model::{NetworkProfileId, OutboundGroupId, OutboundId};
use nonproxy_proto::provider::v1::{DnsRouteKind, ResolveDnsRequest};

use super::DnsServiceError;

const MAXIMUM_QUERY_ID_LENGTH: usize = 128;
const MAXIMUM_UPSTREAMS: usize = 8;

pub struct ValidatedDnsRequest {
    query: ParsedDnsQuery,
    wire_query: Vec<u8>,
    route: RequestedDnsRoute,
    network_profile: Option<NetworkProfileId>,
    upstreams: Vec<SocketAddr>,
    snapshot_version: u64,
    direct_interface_index: Option<NonZeroU32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RequestedDnsRoute {
    Direct,
    System,
    Outbound(OutboundId),
    Group(OutboundGroupId),
}

impl ValidatedDnsRequest {
    pub fn parse(request: ResolveDnsRequest) -> Result<Self, DnsServiceError> {
        validate_query_id(&request.query_id)?;
        validate_app(&request)?;
        if request.snapshot_version == 0 {
            return Err(DnsServiceError::InvalidRequest(
                "snapshot_version 必须大于零",
            ));
        }
        let query =
            ParsedDnsQuery::parse(&request.dns_message).map_err(DnsServiceError::InvalidQuery)?;
        validate_question(&request, &query)?;
        let route = parse_route(&request)?;
        let direct_interface_index = parse_direct_interface(&request, &route)?;
        let network_profile = parse_network_profile(&request.network_profile_id)?;
        let upstreams = parse_upstreams(&request)?;
        Ok(Self {
            query,
            wire_query: request.dns_message,
            route,
            network_profile,
            upstreams,
            snapshot_version: request.snapshot_version,
            direct_interface_index,
        })
    }

    #[must_use]
    pub const fn query(&self) -> &ParsedDnsQuery {
        &self.query
    }

    #[must_use]
    pub fn wire_query(&self) -> &[u8] {
        &self.wire_query
    }

    #[must_use]
    pub const fn route(&self) -> &RequestedDnsRoute {
        &self.route
    }

    #[must_use]
    pub fn upstreams(&self) -> &[SocketAddr] {
        &self.upstreams
    }

    #[must_use]
    pub const fn snapshot_version(&self) -> u64 {
        self.snapshot_version
    }

    #[must_use]
    pub const fn direct_interface_index(&self) -> Option<NonZeroU32> {
        self.direct_interface_index
    }

    #[must_use]
    pub const fn network_profile(&self) -> Option<&NetworkProfileId> {
        self.network_profile.as_ref()
    }
}

fn validate_query_id(value: &str) -> Result<(), DnsServiceError> {
    if value.is_empty()
        || value.len() > MAXIMUM_QUERY_ID_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(DnsServiceError::InvalidRequest("query_id 无效"));
    }
    Ok(())
}

fn validate_app(request: &ResolveDnsRequest) -> Result<(), DnsServiceError> {
    let app = request
        .app
        .as_ref()
        .ok_or(DnsServiceError::InvalidRequest("缺少应用身份"))?;
    if app.stable_id.is_empty() || app.stable_id.len() > 512 {
        return Err(DnsServiceError::InvalidRequest("应用身份无效"));
    }
    Ok(())
}

fn validate_question(
    request: &ResolveDnsRequest,
    query: &ParsedDnsQuery,
) -> Result<(), DnsServiceError> {
    let qname = DnsName::parse_ascii(&request.qname)
        .map_err(|_| DnsServiceError::InvalidRequest("qname 无效"))?;
    if qname != *query.question().qname()
        || request.qtype > u32::from(u16::MAX)
        || request.qtype != u32::from(query.question().qtype())
    {
        return Err(DnsServiceError::InvalidRequest(
            "qname/qtype 与 DNS 消息不一致",
        ));
    }
    Ok(())
}

fn parse_route(request: &ResolveDnsRequest) -> Result<RequestedDnsRoute, DnsServiceError> {
    let route = DnsRouteKind::try_from(request.requested_route)
        .map_err(|_| DnsServiceError::InvalidRequest("DNS route 无效"))?;
    let has_outbound = !request.requested_outbound_id.is_empty();
    let has_group = !request.requested_outbound_group_id.is_empty();
    match (route, has_outbound, has_group) {
        (DnsRouteKind::Direct, false, false) => Ok(RequestedDnsRoute::Direct),
        (DnsRouteKind::System, false, false) => Ok(RequestedDnsRoute::System),
        (DnsRouteKind::Proxy, true, false) => {
            let outbound = OutboundId::new(request.requested_outbound_id.clone())
                .map_err(|_| DnsServiceError::InvalidRequest("DNS outbound_id 无效"))?;
            Ok(RequestedDnsRoute::Outbound(outbound))
        }
        (DnsRouteKind::Proxy, false, true) => {
            let group = OutboundGroupId::new(request.requested_outbound_group_id.clone())
                .map_err(|_| DnsServiceError::InvalidRequest("DNS outbound_group_id 无效"))?;
            Ok(RequestedDnsRoute::Group(group))
        }
        (DnsRouteKind::Unspecified, _, _) => {
            Err(DnsServiceError::InvalidRequest("DNS route 未指定"))
        }
        _ => Err(DnsServiceError::InvalidRequest(
            "DNS route 与代理目标不一致",
        )),
    }
}

fn parse_direct_interface(
    request: &ResolveDnsRequest,
    route: &RequestedDnsRoute,
) -> Result<Option<NonZeroU32>, DnsServiceError> {
    let interface = NonZeroU32::new(request.direct_interface_index);
    match (route, interface) {
        (RequestedDnsRoute::Direct, Some(value)) => Ok(Some(value)),
        (RequestedDnsRoute::Direct, None) => Err(DnsServiceError::InvalidRequest(
            "DIRECT DNS 必须指定物理网卡",
        )),
        (
            RequestedDnsRoute::Outbound(_)
            | RequestedDnsRoute::Group(_)
            | RequestedDnsRoute::System,
            None,
        ) => Ok(None),
        (
            RequestedDnsRoute::Outbound(_)
            | RequestedDnsRoute::Group(_)
            | RequestedDnsRoute::System,
            Some(_),
        ) => Err(DnsServiceError::InvalidRequest(
            "非 DIRECT DNS 不得指定物理网卡",
        )),
    }
}

fn parse_network_profile(value: &str) -> Result<Option<NetworkProfileId>, DnsServiceError> {
    if value.is_empty() {
        return Ok(None);
    }
    NetworkProfileId::new(value)
        .map(Some)
        .map_err(|_| DnsServiceError::InvalidRequest("network_profile_id 无效"))
}

fn parse_upstreams(request: &ResolveDnsRequest) -> Result<Vec<SocketAddr>, DnsServiceError> {
    if request.upstreams.is_empty() || request.upstreams.len() > MAXIMUM_UPSTREAMS {
        return Err(DnsServiceError::InvalidRequest(
            "DNS upstream 数量必须为 1 到 8",
        ));
    }
    let mut unique = HashSet::with_capacity(request.upstreams.len());
    let mut result = Vec::with_capacity(request.upstreams.len());
    for upstream in &request.upstreams {
        let ip = upstream
            .ip_address
            .parse::<IpAddr>()
            .map_err(|_| DnsServiceError::InvalidRequest("DNS upstream 必须是 IP 字面量"))?;
        let port = u16::try_from(upstream.port)
            .ok()
            .filter(|value| *value != 0)
            .ok_or(DnsServiceError::InvalidRequest("DNS upstream 端口无效"))?;
        if ip.is_unspecified() || ip.is_multicast() {
            return Err(DnsServiceError::InvalidRequest("DNS upstream 地址不可路由"));
        }
        let endpoint = match ip {
            IpAddr::V4(address) if upstream.scope_id == 0 => SocketAddr::new(address.into(), port),
            IpAddr::V6(address) => {
                SocketAddr::V6(SocketAddrV6::new(address, port, 0, upstream.scope_id))
            }
            IpAddr::V4(_) => {
                return Err(DnsServiceError::InvalidRequest(
                    "IPv4 DNS upstream 不支持 scope_id",
                ));
            }
        };
        if unique.insert(endpoint) {
            result.push(endpoint);
        }
    }
    Ok(result)
}
