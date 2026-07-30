use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use nonproxy_dns::{
    DnsRoute, ParsedDnsQuery, ParsedDnsResponse, SyntheticAddressFamily, SyntheticAddressSpace,
    address_query,
};
use nonproxy_model::DomainName;
use nonproxy_windows_network::PhysicalDnsCatalog;

use crate::{
    GatewayError,
    dns_service::{DnsResolutionService, DnsServiceError, WireDnsRequest},
};

#[derive(Clone)]
pub struct WindowsDirectDomainResolver {
    resolution: Arc<DnsResolutionService>,
    upstreams: Arc<PhysicalDnsCatalog>,
    address_space: SyntheticAddressSpace,
}

impl WindowsDirectDomainResolver {
    pub const fn new(
        resolution: Arc<DnsResolutionService>,
        upstreams: Arc<PhysicalDnsCatalog>,
        address_space: SyntheticAddressSpace,
    ) -> Self {
        Self {
            resolution,
            upstreams,
            address_space,
        }
    }

    #[must_use]
    pub const fn address_space(&self) -> SyntheticAddressSpace {
        self.address_space
    }

    pub async fn resolve(
        &self,
        domain: &DomainName,
        port: u16,
        snapshot_version: u64,
    ) -> Result<Vec<SocketAddr>, GatewayError> {
        let mut transaction_bytes = [0_u8; 4];
        getrandom::fill(&mut transaction_bytes)
            .map_err(|error| GatewayError::Random(error.to_string()))?;
        let requests = [
            (
                SyntheticAddressFamily::Ipv4,
                u16::from_be_bytes([transaction_bytes[0], transaction_bytes[1]]),
            ),
            (
                SyntheticAddressFamily::Ipv6,
                u16::from_be_bytes([transaction_bytes[2], transaction_bytes[3]]),
            ),
        ];
        let mut addresses = Vec::new();
        let mut unique = HashSet::new();
        for (family, transaction_id) in requests {
            for address in self
                .resolve_family(domain, port, snapshot_version, family, transaction_id)
                .await?
            {
                if unique.insert(address.ip()) {
                    addresses.push(address);
                }
            }
        }
        if addresses.is_empty() {
            Err(GatewayError::WindowsDataPlane(
                "DIRECT 域名没有可用的真实地址".to_owned(),
            ))
        } else {
            Ok(addresses)
        }
    }

    async fn resolve_family(
        &self,
        domain: &DomainName,
        port: u16,
        snapshot_version: u64,
        family: SyntheticAddressFamily,
        transaction_id: u16,
    ) -> Result<Vec<SocketAddr>, GatewayError> {
        let wire_query = address_query(transaction_id, domain, family)
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
        let query = ParsedDnsQuery::parse(&wire_query)
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
        let Ok(response) = resolve_direct_wire(
            self.resolution.as_ref(),
            self.upstreams.as_ref(),
            &query,
            &wire_query,
            snapshot_version,
        )
        .await
        else {
            return Ok(Vec::new());
        };
        let parsed = ParsedDnsResponse::parse(&query, &response)
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
        let mut addresses = Vec::new();
        for observation in parsed.addresses() {
            let address = observation.address();
            let expected_family = matches!(
                (family, address),
                (SyntheticAddressFamily::Ipv4, IpAddr::V4(_))
                    | (SyntheticAddressFamily::Ipv6, IpAddr::V6(_))
            );
            if expected_family
                && !self.address_space.contains(address)
                && !address.is_unspecified()
                && !address.is_multicast()
            {
                addresses.push(SocketAddr::new(address, port));
            }
        }
        Ok(addresses)
    }
}

pub async fn resolve_direct_wire(
    resolution: &DnsResolutionService,
    upstream_catalog: &PhysicalDnsCatalog,
    query: &ParsedDnsQuery,
    wire_query: &[u8],
    snapshot_version: u64,
) -> Result<Vec<u8>, DnsServiceError> {
    let upstreams = upstream_catalog
        .current()
        .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
    let preferred = upstreams.preferred_direct();
    let interface_index = preferred
        .first()
        .map(|value| value.interface_index())
        .ok_or(DnsServiceError::ResolversExhausted)?;
    let endpoints = preferred
        .iter()
        .map(|value| value.endpoint())
        .collect::<Vec<_>>();
    resolution
        .resolve_wire(WireDnsRequest {
            query,
            wire_query,
            route: DnsRoute::Direct,
            upstreams: &endpoints,
            snapshot_version,
            direct_interface_index: Some(interface_index),
            network_profile: None,
        })
        .await
        .map(|result| result.dns_message)
}
