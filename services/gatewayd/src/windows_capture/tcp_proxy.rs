use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_model::{ConnectionContext, Destination, FailureMode, RouteAction, Transport};
use nonproxy_outbound::{OutboundError, TcpDialer};
use nonproxy_policy::{PolicyEngine, PolicyEvaluation};
use nonproxy_windows_identity::WindowsAppIdentityResolver;
use nonproxy_windows_network::PhysicalInterfaceCatalog;
use nonproxy_windows_wfp::query_redirect_metadata;
use tokio::{
    io::{AsyncRead, AsyncWrite, copy_bidirectional},
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
    task::JoinSet,
    time::timeout,
};

use crate::{
    Gateway, GatewayError,
    clock::unix_time_ms,
    credential_store::CredentialStore,
    decision_event::{
        DecisionEventReporter, DecisionObservation, ObservedPath, elapsed_micros, new_flow_id,
    },
    flow_server::outbound_factory::load_connector_with_dialer,
};

use super::{
    dialer::{DirectPathObserver, RedirectTcpDialer},
    direct_dns::WindowsDirectDomainResolver,
    identity::original_remote,
    policy_cache::WindowsPolicyCache,
};

const MAXIMUM_ACTIVE_CONNECTIONS: usize = 2_048;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct WindowsTcpProxy {
    dependencies: TcpConnectionDependencies,
    capacity: Arc<Semaphore>,
}

#[derive(Clone)]
struct TcpConnectionDependencies {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    policies: WindowsPolicyCache,
    physical_interfaces: Arc<PhysicalInterfaceCatalog>,
    direct_domain_resolver: WindowsDirectDomainResolver,
    decisions: DecisionEventReporter,
    application_identities: Arc<WindowsAppIdentityResolver>,
}

impl WindowsTcpProxy {
    pub fn new(
        gateway: Gateway,
        credential_store: Arc<dyn CredentialStore>,
        policies: WindowsPolicyCache,
        physical_interfaces: Arc<PhysicalInterfaceCatalog>,
        direct_domain_resolver: WindowsDirectDomainResolver,
        decisions: DecisionEventReporter,
        application_identities: Arc<WindowsAppIdentityResolver>,
    ) -> Self {
        Self {
            dependencies: TcpConnectionDependencies {
                gateway,
                credential_store,
                policies,
                physical_interfaces,
                direct_domain_resolver,
                decisions,
                application_identities,
            },
            capacity: Arc::new(Semaphore::new(MAXIMUM_ACTIVE_CONNECTIONS)),
        }
    }

    pub async fn serve(
        self,
        ipv4: TcpListener,
        ipv6: TcpListener,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), GatewayError> {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                accepted = ipv4.accept() => {
                    let (stream, _) = accepted.map_err(|source| GatewayError::Io {
                        operation: "接收 Windows IPv4 重定向连接",
                        source,
                    })?;
                    self.spawn(stream, &mut tasks);
                }
                accepted = ipv6.accept() => {
                    let (stream, _) = accepted.map_err(|source| GatewayError::Io {
                        operation: "接收 Windows IPv6 重定向连接",
                        source,
                    })?;
                    self.spawn(stream, &mut tasks);
                }
                completed = tasks.join_next(), if !tasks.is_empty() => {
                    let _completed = completed;
                }
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        Ok(())
    }

    fn spawn(&self, stream: TcpStream, tasks: &mut JoinSet<()>) {
        let Ok(permit) = Arc::clone(&self.capacity).try_acquire_owned() else {
            drop(stream);
            return;
        };
        let dependencies = self.dependencies.clone();
        tasks.spawn(async move {
            let _permit = permit;
            let _result = handle_connection(stream, dependencies).await;
        });
    }
}

async fn handle_connection(
    mut inbound: TcpStream,
    dependencies: TcpConnectionDependencies,
) -> Result<(), GatewayError> {
    let TcpConnectionDependencies {
        gateway,
        credential_store,
        policies,
        physical_interfaces,
        direct_domain_resolver,
        decisions,
        application_identities,
    } = dependencies;
    let metadata =
        query_redirect_metadata(std::os::windows::io::AsRawSocket::as_raw_socket(&inbound))
            .map_err(data_plane_error)?;
    let remote = original_remote(metadata.context())?;
    let (target, destination) = if direct_domain_resolver.address_space().contains(remote.ip()) {
        let binding = gateway
            .synthetic_dns_lookup(direct_domain_resolver.address_space(), remote.ip())
            .await?
            .ok_or_else(|| {
                GatewayError::WindowsDataPlane("合成 DNS 目标没有有效域名绑定".to_owned())
            })?;
        (
            FlowEndpoint::Domain(binding.domain().clone(), remote.port()),
            Destination::new(
                Some(binding.domain().as_ascii()),
                None,
                remote.port(),
                Transport::Tcp,
            )?,
        )
    } else {
        (
            FlowEndpoint::Ip(remote),
            Destination::new(None, Some(remote.ip()), remote.port(), Transport::Tcp)?,
        )
    };
    let app = application_identities
        .resolve(metadata.context().app_id(), metadata.context().process_id())
        .await;
    let context = ConnectionContext::new(app, destination);
    let snapshot = policies
        .current()
        .await
        .ok_or_else(|| GatewayError::WindowsDataPlane("没有可用的活动策略快照".to_owned()))?;
    let observed_at_unix_ms = unix_time_ms()?;
    let decision_started = Instant::now();
    let evaluation = PolicyEngine::evaluate_at(&snapshot, &context, observed_at_unix_ms);
    let decision_latency_micros = elapsed_micros(decision_started);
    if let PolicyEvaluation::Bypass { .. } = evaluation {
        let dialer = RedirectTcpDialer::new(metadata.records());
        let mut outbound = dialer.connect(&target).await.map_err(data_plane_error)?;
        return relay(&mut inbound, &mut outbound).await;
    }
    let PolicyEvaluation::Decision(decision) = evaluation else {
        unreachable!("旁路判定已提前返回")
    };
    let observation = DecisionObservation::new(
        "windows-wfp",
        policies.provider_generation(),
        new_flow_id("tcp"),
        observed_at_unix_ms,
        context,
        decision.clone(),
        decision_latency_micros,
    );
    let proxy_dialer: Arc<dyn TcpDialer> = Arc::new(RedirectTcpDialer::new(metadata.records()));
    let (direct_dialer, direct_path) =
        RedirectTcpDialer::direct(metadata.records(), physical_interfaces);
    let direct_dialer: Arc<dyn TcpDialer> = Arc::new(direct_dialer);
    match decision.result().action() {
        RouteAction::Block => {
            report(&decisions, &observation, ObservedPath::Decision, None);
            Ok(())
        }
        RouteAction::Direct => {
            let connected = connect_direct(
                Arc::clone(&direct_dialer),
                &target,
                &direct_domain_resolver,
                snapshot.metadata().snapshot_version(),
            )
            .await;
            let mut outbound = match connected {
                Ok(value) => value,
                Err(error) => {
                    report(
                        &decisions,
                        &observation,
                        ObservedPath::Decision,
                        Some("NP_WINDOWS_DIRECT_CONNECT_FAILED"),
                    );
                    return Err(error);
                }
            };
            report_direct(&decisions, &observation, &direct_path, false);
            relay(&mut inbound, &mut outbound).await
        }
        RouteAction::Proxy => {
            let outbound_id = decision
                .result()
                .outbound_id()
                .ok_or(GatewayError::InvalidContract("代理决策缺少出口"))?;
            let proxied = async {
                let connector = load_connector_with_dialer(
                    &gateway,
                    Arc::clone(&credential_store),
                    outbound_id,
                    proxy_dialer,
                )
                .await
                .map_err(data_plane_error)?;
                connector
                    .connect_tcp(&target)
                    .await
                    .map_err(data_plane_error)
            }
            .await;
            match proxied {
                Ok(mut outbound) => {
                    report(
                        &decisions,
                        &observation,
                        ObservedPath::Proxy {
                            outbound_id: outbound_id.clone(),
                        },
                        None,
                    );
                    relay(&mut inbound, &mut outbound).await
                }
                Err(_) if decision.result().failure_mode() == FailureMode::Open => {
                    let connected = connect_direct(
                        direct_dialer,
                        &target,
                        &direct_domain_resolver,
                        snapshot.metadata().snapshot_version(),
                    )
                    .await;
                    let mut outbound = match connected {
                        Ok(value) => value,
                        Err(error) => {
                            report(
                                &decisions,
                                &observation,
                                ObservedPath::Decision,
                                Some("NP_WINDOWS_PROXY_FAIL_OPEN_FAILED"),
                            );
                            return Err(error);
                        }
                    };
                    report_direct(&decisions, &observation, &direct_path, true);
                    relay(&mut inbound, &mut outbound).await
                }
                Err(error) => {
                    report(
                        &decisions,
                        &observation,
                        ObservedPath::Decision,
                        Some("NP_WINDOWS_PROXY_CONNECT_FAILED"),
                    );
                    Err(error)
                }
            }
        }
    }
}

fn report_direct(
    reporter: &DecisionEventReporter,
    observation: &DecisionObservation,
    path: &DirectPathObserver,
    fail_open: bool,
) {
    let Some(interface_index) = path.interface_index() else {
        report(
            reporter,
            observation,
            ObservedPath::Decision,
            Some("NP_WINDOWS_DIRECT_INTERFACE_UNKNOWN"),
        );
        return;
    };
    report(
        reporter,
        observation,
        ObservedPath::Direct {
            interface_index,
            fail_open,
        },
        fail_open.then_some("NP_WINDOWS_PROXY_FAIL_OPEN_DIRECT"),
    );
}

fn report(
    reporter: &DecisionEventReporter,
    observation: &DecisionObservation,
    path: ObservedPath,
    error_code: Option<&str>,
) {
    match observation.record(path, error_code) {
        Ok(decision) => {
            let _accepted = reporter.submit(decision);
        }
        Err(_) => reporter.record_unreportable(),
    }
}

async fn connect_direct(
    dialer: Arc<dyn TcpDialer>,
    target: &FlowEndpoint,
    domain_resolver: &WindowsDirectDomainResolver,
    snapshot_version: u64,
) -> Result<TcpStream, GatewayError> {
    match target {
        FlowEndpoint::Ip(_) => connect_direct_endpoint(dialer, target).await,
        FlowEndpoint::Domain(domain, port) => {
            let addresses = domain_resolver
                .resolve(domain, *port, snapshot_version)
                .await?;
            let mut last_error = None;
            for address in addresses {
                match connect_direct_endpoint(Arc::clone(&dialer), &FlowEndpoint::Ip(address)).await
                {
                    Ok(stream) => return Ok(stream),
                    Err(error) => last_error = Some(error),
                }
            }
            Err(last_error.unwrap_or_else(|| {
                GatewayError::WindowsDataPlane("DIRECT 域名没有可连接地址".to_owned())
            }))
        }
    }
}

async fn connect_direct_endpoint(
    dialer: Arc<dyn TcpDialer>,
    target: &FlowEndpoint,
) -> Result<TcpStream, GatewayError> {
    timeout(CONNECT_TIMEOUT, dialer.connect(target))
        .await
        .map_err(|_| data_plane_error(OutboundError::ConnectTimeout))?
        .map_err(data_plane_error)
}

async fn relay<T>(inbound: &mut TcpStream, outbound: &mut T) -> Result<(), GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    copy_bidirectional(inbound, outbound)
        .await
        .map(|_| ())
        .map_err(|source| GatewayError::Io {
            operation: "转发 Windows 重定向 TCP",
            source,
        })
}

fn data_plane_error(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::WindowsDataPlane(error.to_string())
}
