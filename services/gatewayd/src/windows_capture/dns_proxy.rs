use std::{
    net::{Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::Duration,
};

use nonproxy_dns::{
    DnsRoute, ParsedDnsQuery, SyntheticAddressSpace, refused_response, synthetic_address_response,
    synthetic_nodata_response,
};
use nonproxy_model::{FailureMode, RouteAction};
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_windows_network::{
    PhysicalDnsCatalog, PhysicalInterfaceCatalog, ensure_synthetic_ipv4_pool_available,
    verify_system_dns_probe,
};
use tokio::{
    sync::watch,
    time::{MissedTickBehavior, interval},
};

use crate::{
    Gateway, GatewayError,
    credential_store::CredentialStore,
    dns_policy::{DnsQueryPlan, plan_query},
    dns_service::{DnsResolutionService, DnsServiceError, WireDnsRequest},
    local_dns_server::{LocalDnsQueryProcessor, LocalDnsServer, ProcessingFuture},
};

use super::direct_dns::{WindowsDirectDomainResolver, resolve_direct_wire};
use super::policy_cache::WindowsPolicyCache;

const LOCAL_DNS_PORT: u16 = 53;
const PROBE_ADDRESS: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
const READINESS_INTERVAL: Duration = Duration::from_secs(2);

pub struct WindowsDnsProxy {
    server: LocalDnsServer,
    processor: Arc<WindowsDnsProcessor>,
    policies: WindowsPolicyCache,
    readiness_sender: watch::Sender<bool>,
}

struct WindowsDnsProcessor {
    gateway: Gateway,
    resolution: Arc<DnsResolutionService>,
    policies: WindowsPolicyCache,
    upstreams: Arc<PhysicalDnsCatalog>,
    address_space: SyntheticAddressSpace,
    probe_domain: String,
}

impl WindowsDnsProxy {
    pub async fn start(
        gateway: Gateway,
        credential_store: Arc<dyn CredentialStore>,
        physical_interfaces: Arc<PhysicalInterfaceCatalog>,
    ) -> Result<(Self, watch::Receiver<bool>, WindowsDirectDomainResolver), GatewayError> {
        ensure_synthetic_ipv4_pool_available()
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
        let policies = WindowsPolicyCache::load(gateway.clone(), "windows-dns", false).await?;
        let address_space = gateway
            .load_or_create_synthetic_dns_space(random_ula_prefix()?)
            .await?;
        let upstreams = Arc::new(PhysicalDnsCatalog::new(physical_interfaces));
        upstreams
            .current()
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
        let server = LocalDnsServer::bind_loopback(LOCAL_DNS_PORT).await?;
        let probe_domain = random_probe_domain()?;
        let resolution = Arc::new(DnsResolutionService::new(gateway.clone(), credential_store));
        let processor = Arc::new(WindowsDnsProcessor {
            resolution: Arc::clone(&resolution),
            gateway,
            policies: policies.clone(),
            upstreams: Arc::clone(&upstreams),
            address_space,
            probe_domain,
        });
        let direct_resolver =
            WindowsDirectDomainResolver::new(resolution, upstreams, address_space);
        let (readiness_sender, readiness_receiver) = watch::channel(false);
        Ok((
            Self {
                server,
                processor,
                policies,
                readiness_sender,
            },
            readiness_receiver,
            direct_resolver,
        ))
    }

    pub async fn serve(self, mut shutdown: watch::Receiver<bool>) -> Result<(), GatewayError> {
        let Self {
            server,
            processor,
            policies,
            readiness_sender,
        } = self;
        let (worker_stop_sender, worker_stop_receiver) = watch::channel(false);
        let server_worker = server.serve(
            Arc::clone(&processor) as Arc<dyn LocalDnsQueryProcessor>,
            worker_stop_receiver.clone(),
        );
        let policy_worker = policies
            .clone()
            .refresh_until_shutdown(worker_stop_receiver.clone());
        let readiness_worker = readiness_loop(
            Arc::clone(&processor),
            policies,
            readiness_sender,
            worker_stop_receiver,
        );
        tokio::pin!(server_worker);
        tokio::pin!(policy_worker);
        tokio::pin!(readiness_worker);
        let first = tokio::select! {
            changed = shutdown.changed() => {
                let _changed = changed;
                FirstCompletion::Shutdown
            }
            result = &mut server_worker => FirstCompletion::Server(result),
            result = &mut policy_worker => FirstCompletion::Policy(result),
            result = &mut readiness_worker => FirstCompletion::Readiness(result),
        };
        let _stop_result = worker_stop_sender.send(true);
        match first {
            FirstCompletion::Shutdown => {
                let (server_result, policy_result, readiness_result) = tokio::join!(
                    &mut server_worker,
                    &mut policy_worker,
                    &mut readiness_worker
                );
                server_result?;
                policy_result?;
                readiness_result
            }
            FirstCompletion::Server(result) => {
                let (policy_result, readiness_result) =
                    tokio::join!(&mut policy_worker, &mut readiness_worker);
                result?;
                policy_result?;
                readiness_result
            }
            FirstCompletion::Policy(result) => {
                let (server_result, readiness_result) =
                    tokio::join!(&mut server_worker, &mut readiness_worker);
                result?;
                server_result?;
                readiness_result
            }
            FirstCompletion::Readiness(result) => {
                let (server_result, policy_result) =
                    tokio::join!(&mut server_worker, &mut policy_worker);
                result?;
                server_result?;
                policy_result
            }
        }
    }
}

impl LocalDnsQueryProcessor for WindowsDnsProcessor {
    fn process(&self, query: Vec<u8>) -> ProcessingFuture<'_> {
        Box::pin(self.process_query(query))
    }
}

impl WindowsDnsProcessor {
    async fn process_query(&self, wire_query: Vec<u8>) -> Result<Vec<u8>, DnsServiceError> {
        let query = ParsedDnsQuery::parse(&wire_query).map_err(DnsServiceError::InvalidQuery)?;
        if query.question().qtype() == 1 && query.question().qname().as_ascii() == self.probe_domain
        {
            return synthetic_address_response(&wire_query, PROBE_ADDRESS.into())
                .map_err(DnsServiceError::InvalidQuery);
        }
        let snapshot = self
            .policies
            .current()
            .await
            .ok_or(DnsServiceError::SnapshotUnavailable)?;
        match plan_query(&snapshot, &query) {
            DnsQueryPlan::Synthetic { domain, family } => {
                let binding = self
                    .gateway
                    .synthetic_dns_binding(self.address_space, domain, family)
                    .await?;
                synthetic_address_response(&wire_query, binding.address())
                    .map_err(DnsServiceError::InvalidQuery)
            }
            DnsQueryPlan::NoData => {
                synthetic_nodata_response(&wire_query).map_err(DnsServiceError::InvalidQuery)
            }
            DnsQueryPlan::Route(decision) => match decision.action() {
                RouteAction::Block => {
                    refused_response(&wire_query).map_err(DnsServiceError::InvalidQuery)
                }
                RouteAction::Direct => self.resolve_direct(&query, &wire_query, &snapshot).await,
                RouteAction::Proxy => {
                    let proxy = self
                        .resolve_proxy(&query, &wire_query, &snapshot, &decision)
                        .await;
                    if proxy.is_err() && decision.failure_mode() == FailureMode::Open {
                        self.resolve_direct(&query, &wire_query, &snapshot).await
                    } else {
                        proxy
                    }
                }
            },
        }
    }

    async fn resolve_direct(
        &self,
        query: &ParsedDnsQuery,
        wire_query: &[u8],
        snapshot: &nonproxy_policy::CompiledPolicySnapshot,
    ) -> Result<Vec<u8>, DnsServiceError> {
        resolve_direct_wire(
            self.resolution.as_ref(),
            self.upstreams.as_ref(),
            query,
            wire_query,
            snapshot.metadata().snapshot_version(),
        )
        .await
    }

    async fn resolve_proxy(
        &self,
        query: &ParsedDnsQuery,
        wire_query: &[u8],
        snapshot: &nonproxy_policy::CompiledPolicySnapshot,
        decision: &nonproxy_model::DecisionSpec,
    ) -> Result<Vec<u8>, DnsServiceError> {
        let upstreams = self
            .upstreams
            .current()
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?
            .all_endpoints();
        let outbound = decision
            .outbound_id()
            .ok_or(DnsServiceError::InvalidRequest("代理 DNS 缺少出口"))?
            .clone();
        self.resolution
            .resolve_wire(WireDnsRequest {
                query,
                wire_query,
                route: DnsRoute::Proxy(outbound),
                upstreams: &upstreams,
                snapshot_version: snapshot.metadata().snapshot_version(),
                direct_interface_index: None,
                network_profile: None,
            })
            .await
            .map(|result| result.dns_message)
    }
}

async fn readiness_loop(
    processor: Arc<WindowsDnsProcessor>,
    policies: WindowsPolicyCache,
    readiness: watch::Sender<bool>,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), GatewayError> {
    let mut ticker = interval(READINESS_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    let _send_result = readiness.send(false);
                    policies.disable_acknowledgements().await;
                    return Ok(());
                }
            }
            _ = ticker.tick() => {
                let probe_domain = processor.probe_domain.clone();
                let probe = tokio::task::spawn_blocking(move || {
                    verify_system_dns_probe(&probe_domain, PROBE_ADDRESS)
                })
                .await
                .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?;
                let ready = probe.is_ok();
                let _send_result = readiness.send(ready);
                if ready {
                    policies.enable_acknowledgements().await?;
                } else {
                    policies.disable_acknowledgements().await;
                }
                let current_version = policies
                    .current()
                    .await
                    .map_or(0, |snapshot| snapshot.metadata().snapshot_version());
                let state = if ready && current_version != 0 {
                    RuntimeState::Ready
                } else if ready {
                    RuntimeState::Starting
                } else {
                    RuntimeState::Degraded
                };
                policies.report_health(state, current_version)?;
            }
        }
    }
}

fn random_ula_prefix() -> Result<Ipv6Addr, GatewayError> {
    let mut octets = [0_u8; 16];
    getrandom::fill(&mut octets[..8]).map_err(|error| GatewayError::Random(error.to_string()))?;
    octets[0] = 0xfd;
    Ok(Ipv6Addr::from(octets))
}

fn random_probe_domain() -> Result<String, GatewayError> {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| GatewayError::Random(error.to_string()))?;
    let encoded = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{encoded}.probe.nonproxy.invalid"))
}

enum FirstCompletion {
    Shutdown,
    Server(Result<(), GatewayError>),
    Policy(Result<(), GatewayError>),
    Readiness(Result<(), GatewayError>),
}
