use std::{
    net::Ipv4Addr,
    sync::Arc,
    time::{Duration, Instant},
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
    clock::unix_time_ms,
    credential_store::CredentialStore,
    decision_event::{DecisionEventReporter, elapsed_micros},
    dns_policy::{DnsQueryPlan, plan_query_at},
    dns_service::{DnsResolutionResult, DnsResolutionService, DnsServiceError, WireDnsRequest},
    local_dns_server::{LocalDnsQueryProcessor, LocalDnsServer, ProcessingFuture},
};

use super::direct_dns::{DirectDnsResolution, WindowsDirectDomainResolver, resolve_direct_path};
use super::dns_decision::WindowsDnsObservation;
use super::dns_runtime::{DnsFirstCompletion, random_probe_domain, random_ula_prefix};
use super::policy_cache::WindowsPolicyCache;

const PROBE_ADDRESS: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
const READINESS_INTERVAL: Duration = Duration::from_secs(2);

pub struct WindowsDnsProxy {
    server: LocalDnsServer,
    processor: Arc<WindowsDnsProcessor>,
    policies: WindowsPolicyCache,
    readiness_sender: watch::Sender<bool>,
    redirect_ports: (u16, u16),
}

struct WindowsDnsProcessor {
    gateway: Gateway,
    resolution: Arc<DnsResolutionService>,
    policies: WindowsPolicyCache,
    upstreams: Arc<PhysicalDnsCatalog>,
    address_space: SyntheticAddressSpace,
    probe_domain: String,
    decisions: DecisionEventReporter,
}

impl WindowsDnsProxy {
    pub async fn start(
        gateway: Gateway,
        credential_store: Arc<dyn CredentialStore>,
        physical_interfaces: Arc<PhysicalInterfaceCatalog>,
        decisions: DecisionEventReporter,
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
        let server = LocalDnsServer::bind_loopback(0).await?;
        let redirect_ports = server.ports();
        let probe_domain = random_probe_domain()?;
        let resolution = Arc::new(DnsResolutionService::new(gateway.clone(), credential_store));
        let processor = Arc::new(WindowsDnsProcessor {
            resolution: Arc::clone(&resolution),
            gateway,
            policies: policies.clone(),
            upstreams: Arc::clone(&upstreams),
            address_space,
            probe_domain,
            decisions,
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
                redirect_ports,
            },
            readiness_receiver,
            direct_resolver,
        ))
    }

    #[must_use]
    pub const fn redirect_ports(&self) -> (u16, u16) {
        self.redirect_ports
    }

    pub async fn serve(self, mut shutdown: watch::Receiver<bool>) -> Result<(), GatewayError> {
        let Self {
            server,
            processor,
            policies,
            readiness_sender,
            redirect_ports: _,
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
                DnsFirstCompletion::Shutdown
            }
            result = &mut server_worker => DnsFirstCompletion::Server(result),
            result = &mut policy_worker => DnsFirstCompletion::Policy(result),
            result = &mut readiness_worker => DnsFirstCompletion::Readiness(result),
        };
        let _stop_result = worker_stop_sender.send(true);
        match first {
            DnsFirstCompletion::Shutdown => {
                let (server_result, policy_result, readiness_result) = tokio::join!(
                    &mut server_worker,
                    &mut policy_worker,
                    &mut readiness_worker
                );
                server_result?;
                policy_result?;
                readiness_result
            }
            DnsFirstCompletion::Server(result) => {
                let (policy_result, readiness_result) =
                    tokio::join!(&mut policy_worker, &mut readiness_worker);
                result?;
                policy_result?;
                readiness_result
            }
            DnsFirstCompletion::Policy(result) => {
                let (server_result, readiness_result) =
                    tokio::join!(&mut server_worker, &mut readiness_worker);
                result?;
                server_result?;
                readiness_result
            }
            DnsFirstCompletion::Readiness(result) => {
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
        let Some(snapshot) = self.policies.current().await else {
            return self
                .resolve_direct(&query, &wire_query, 0)
                .await
                .map(|result| result.dns_message);
        };
        let observed_at_unix_ms = unix_time_ms()?;
        let decision_started = Instant::now();
        let plan = plan_query_at(&snapshot, &query, observed_at_unix_ms);
        let decision_latency_micros = elapsed_micros(decision_started);
        match plan {
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
            DnsQueryPlan::System { snapshot_version } => self
                .resolve_system(&query, &wire_query, snapshot_version)
                .await
                .map(|result| result.dns_message),
            DnsQueryPlan::Route(route) => {
                let route = *route;
                let decision = route.decision;
                let observation = WindowsDnsObservation::new(
                    self.decisions.clone(),
                    self.policies.provider_generation(),
                    route.context,
                    decision.clone(),
                    observed_at_unix_ms,
                    decision_latency_micros,
                );
                match decision.result().action() {
                    RouteAction::Block => {
                        observation.decision();
                        refused_response(&wire_query).map_err(DnsServiceError::InvalidQuery)
                    }
                    RouteAction::Direct => {
                        let resolved = self
                            .resolve_direct(
                                &query,
                                &wire_query,
                                snapshot.metadata().snapshot_version(),
                            )
                            .await;
                        match resolved {
                            Ok(result) => {
                                observation.direct(result.interface_index, result.cache_hit, false);
                                Ok(result.dns_message)
                            }
                            Err(error) => {
                                observation.failed("NP_WINDOWS_DNS_DIRECT_FAILED");
                                Err(error)
                            }
                        }
                    }
                    RouteAction::Proxy => {
                        let proxy = self
                            .resolve_proxy(&query, &wire_query, &snapshot, &decision)
                            .await;
                        match proxy {
                            Ok(result) => {
                                let outbound_id = decision
                                    .result()
                                    .outbound_id()
                                    .ok_or(DnsServiceError::InvalidRequest("代理 DNS 缺少出口"))?
                                    .clone();
                                observation.proxy(outbound_id, result.cache_hit);
                                Ok(result.dns_message)
                            }
                            Err(_proxy_error)
                                if decision.result().failure_mode() == FailureMode::Open =>
                            {
                                let direct = self
                                    .resolve_direct(
                                        &query,
                                        &wire_query,
                                        snapshot.metadata().snapshot_version(),
                                    )
                                    .await;
                                match direct {
                                    Ok(result) => {
                                        observation.direct(
                                            result.interface_index,
                                            result.cache_hit,
                                            true,
                                        );
                                        Ok(result.dns_message)
                                    }
                                    Err(error) => {
                                        observation.failed("NP_WINDOWS_DNS_PROXY_FAIL_OPEN_FAILED");
                                        Err(error)
                                    }
                                }
                            }
                            Err(error) => {
                                observation.failed("NP_WINDOWS_DNS_PROXY_FAILED");
                                Err(error)
                            }
                        }
                    }
                }
            }
        }
    }

    async fn resolve_direct(
        &self,
        query: &ParsedDnsQuery,
        wire_query: &[u8],
        snapshot_version: u64,
    ) -> Result<DirectDnsResolution, DnsServiceError> {
        resolve_direct_path(
            self.resolution.as_ref(),
            self.upstreams.as_ref(),
            query,
            wire_query,
            snapshot_version,
        )
        .await
    }

    async fn resolve_proxy(
        &self,
        query: &ParsedDnsQuery,
        wire_query: &[u8],
        snapshot: &nonproxy_policy::CompiledPolicySnapshot,
        decision: &nonproxy_model::Decision,
    ) -> Result<DnsResolutionResult, DnsServiceError> {
        let upstreams = self
            .upstreams
            .current()
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?
            .all_endpoints();
        let outbound = decision
            .result()
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
    }

    async fn resolve_system(
        &self,
        query: &ParsedDnsQuery,
        wire_query: &[u8],
        snapshot_version: u64,
    ) -> Result<DnsResolutionResult, DnsServiceError> {
        let upstreams = self
            .upstreams
            .current()
            .map_err(|error| GatewayError::WindowsDataPlane(error.to_string()))?
            .all_endpoints();
        self.resolution
            .resolve_wire(WireDnsRequest {
                query,
                wire_query,
                route: DnsRoute::System,
                upstreams: &upstreams,
                snapshot_version,
                direct_interface_index: None,
                network_profile: None,
            })
            .await
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
