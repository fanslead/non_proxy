mod error;
mod request;
mod resolver;

use std::{sync::Arc, time::Duration};

use nonproxy_dns::{DnsRoute, ParsedDnsQuery, ParsedDnsResponse, PartitionedDnsCache};
use nonproxy_model::{NetworkProfileId, OutboundId};
use nonproxy_proto::{
    common::v1::ErrorDetail,
    provider::v1::{DnsRouteKind, ResolveDnsRequest, ResolveDnsResponse},
};
use prost_types::Duration as ProtoDuration;
use tokio::time::timeout;

use crate::{
    Gateway, clock::unix_time_ms, credential_store::CredentialStore,
    flow_server::outbound_factory::load_connector,
};

pub use error::DnsServiceError;
use request::{RequestedDnsRoute, ValidatedDnsRequest};

const DNS_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct DnsResolutionService {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    cache: Arc<PartitionedDnsCache>,
}

pub struct DnsResolutionResult {
    pub dns_message: Vec<u8>,
    pub route: DnsRoute,
    pub valid_for_seconds: u32,
    pub cache_hit: bool,
    pub resolver_endpoint: Option<String>,
}

pub(crate) struct WireDnsRequest<'request> {
    pub query: &'request ParsedDnsQuery,
    pub wire_query: &'request [u8],
    pub route: DnsRoute,
    pub upstreams: &'request [std::net::SocketAddr],
    pub snapshot_version: u64,
    pub direct_interface_index: Option<std::num::NonZeroU32>,
    pub network_profile: Option<&'request NetworkProfileId>,
}

impl DnsResolutionService {
    #[must_use]
    pub fn new(gateway: Gateway, credential_store: Arc<dyn CredentialStore>) -> Self {
        Self {
            gateway,
            credential_store,
            cache: Arc::new(PartitionedDnsCache::default()),
        }
    }

    pub async fn resolve(
        &self,
        request: ResolveDnsRequest,
    ) -> Result<DnsResolutionResult, DnsServiceError> {
        let request = ValidatedDnsRequest::parse(request)?;
        let route = match request.route() {
            RequestedDnsRoute::Direct => DnsRoute::Direct,
            RequestedDnsRoute::System => DnsRoute::System,
            RequestedDnsRoute::Outbound(id) => DnsRoute::Proxy(id.clone()),
            RequestedDnsRoute::Group(id) => DnsRoute::Proxy(
                self.gateway
                    .select_outbound_group(request.snapshot_version(), id)
                    .await?
                    .outbound_id()
                    .clone(),
            ),
        };
        self.resolve_wire(WireDnsRequest {
            query: request.query(),
            wire_query: request.wire_query(),
            route,
            upstreams: request.upstreams(),
            snapshot_version: request.snapshot_version(),
            direct_interface_index: request.direct_interface_index(),
            network_profile: request.network_profile(),
        })
        .await
    }

    pub(crate) async fn resolve_wire(
        &self,
        request: WireDnsRequest<'_>,
    ) -> Result<DnsResolutionResult, DnsServiceError> {
        if self.gateway.active_snapshot_version().await? != Some(request.snapshot_version) {
            return Err(DnsServiceError::SnapshotUnavailable);
        }
        let key = request
            .query
            .cache_key(request.route.clone(), request.network_profile.cloned());
        let now = unix_time_ms()?;
        if let Some(cached) = self
            .cache
            .get(&key, request.query.transaction_id(), now)
            .map_err(DnsServiceError::Cache)?
        {
            return Ok(DnsResolutionResult {
                dns_message: cached.bytes().to_vec(),
                route: request.route,
                valid_for_seconds: cached.remaining_ttl_seconds(),
                cache_hit: true,
                resolver_endpoint: None,
            });
        }

        let forwarded = timeout(DNS_REQUEST_TIMEOUT, async {
            match &request.route {
                DnsRoute::Direct | DnsRoute::System => {
                    resolver::direct(
                        request.upstreams,
                        request.query,
                        request.wire_query,
                        request.direct_interface_index,
                    )
                    .await
                }
                DnsRoute::Proxy(outbound_id) => {
                    let connector = load_connector(
                        &self.gateway,
                        Arc::clone(&self.credential_store),
                        outbound_id,
                    )
                    .await?;
                    resolver::proxy(
                        &connector,
                        request.upstreams,
                        request.query,
                        request.wire_query,
                    )
                    .await
                }
            }
        })
        .await
        .map_err(|_| DnsServiceError::ResolverTimeout)??;
        let response = ParsedDnsResponse::parse(request.query, &forwarded.bytes)
            .map_err(DnsServiceError::InvalidResponse)?;
        let valid_for_seconds = response.valid_for_seconds();
        self.cache
            .insert(key, response, now)
            .map_err(DnsServiceError::Cache)?;
        Ok(DnsResolutionResult {
            dns_message: forwarded.bytes,
            route: request.route,
            valid_for_seconds,
            cache_hit: false,
            resolver_endpoint: Some(forwarded.resolver.to_string()),
        })
    }
}

impl DnsResolutionResult {
    #[must_use]
    pub const fn outbound_id(&self) -> Option<&OutboundId> {
        match &self.route {
            DnsRoute::Proxy(value) => Some(value),
            DnsRoute::Direct | DnsRoute::System => None,
        }
    }
}

pub(crate) fn response(result: DnsResolutionResult) -> ResolveDnsResponse {
    let route = match &result.route {
        DnsRoute::Direct => DnsRouteKind::Direct,
        DnsRoute::Proxy(_) => DnsRouteKind::Proxy,
        DnsRoute::System => DnsRouteKind::System,
    };
    let outbound_id = result
        .outbound_id()
        .map_or_else(String::new, ToString::to_string);
    ResolveDnsResponse {
        dns_message: result.dns_message,
        route: route as i32,
        outbound_id,
        valid_for: Some(ProtoDuration {
            seconds: i64::from(result.valid_for_seconds),
            nanos: 0,
        }),
        error: None,
        cache_hit: result.cache_hit,
        resolver_endpoint: result.resolver_endpoint.unwrap_or_default(),
    }
}

pub(crate) fn error_response(error: &DnsServiceError) -> ResolveDnsResponse {
    ResolveDnsResponse {
        dns_message: Vec::new(),
        route: DnsRouteKind::Unspecified as i32,
        outbound_id: String::new(),
        valid_for: None,
        error: Some(ErrorDetail {
            code: error.code().to_owned(),
            message: error.to_string(),
            retryable: error.retryable(),
            metadata: Default::default(),
        }),
        cache_hit: false,
        resolver_endpoint: String::new(),
    }
}

#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod tests;
