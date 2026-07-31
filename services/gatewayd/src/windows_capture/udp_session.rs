use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_model::{ConnectionContext, Destination, FailureMode, RouteAction, Transport};
use nonproxy_outbound::{Socks5UdpAssociation, SystemTcpDialer};
use nonproxy_policy::PolicyEngine;
use nonproxy_windows_network::PhysicalInterfaceCatalog;
use nonproxy_windows_wfp::{MAX_UDP_PAYLOAD_BYTES, UdpDatagram, UdpInjectionContext};
use tokio::{
    net::UdpSocket,
    sync::{OwnedSemaphorePermit, mpsc},
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
    direct_dns::WindowsDirectDomainResolver, identity::app_identity_from_bytes,
    policy_cache::WindowsPolicyCache, udp_direct::connect_direct_udp, udp_driver::UdpInjector,
};

const SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

pub struct PendingUdpPayload {
    bytes: Vec<u8>,
    _budget: OwnedSemaphorePermit,
}

impl PendingUdpPayload {
    pub const fn new(bytes: Vec<u8>, budget: OwnedSemaphorePermit) -> Self {
        Self {
            bytes,
            _budget: budget,
        }
    }
}

#[derive(Clone)]
pub struct UdpSessionDependencies {
    pub gateway: Gateway,
    pub credential_store: Arc<dyn CredentialStore>,
    pub policies: WindowsPolicyCache,
    pub physical_interfaces: Arc<PhysicalInterfaceCatalog>,
    pub direct_domain_resolver: WindowsDirectDomainResolver,
    pub injector: UdpInjector,
    pub decisions: DecisionEventReporter,
}

pub async fn run_udp_session(
    first: UdpDatagram,
    first_budget: OwnedSemaphorePermit,
    mut incoming: mpsc::Receiver<PendingUdpPayload>,
    dependencies: UdpSessionDependencies,
) -> Result<(), GatewayError> {
    let exposed_remote = first.remote();
    let injection = first.injection_context();
    let first_payload = PendingUdpPayload::new(first.payload().to_vec(), first_budget);
    let (target, destination) = session_target(&first, &dependencies).await?;
    let context = ConnectionContext::new(app_identity_from_bytes(first.app_id()), destination);
    drop(first);
    let snapshot = dependencies
        .policies
        .current()
        .await
        .ok_or_else(|| GatewayError::WindowsDataPlane("没有可用的活动策略快照".to_owned()))?;
    let observed_at_unix_ms = unix_time_ms()?;
    let decision_started = Instant::now();
    let decision = PolicyEngine::decide(&snapshot, &context);
    let decision_latency_micros = elapsed_micros(decision_started);
    let observation = DecisionObservation::new(
        "windows-wfp",
        dependencies.policies.provider_generation(),
        new_flow_id("udp"),
        observed_at_unix_ms,
        context,
        decision.clone(),
        decision_latency_micros,
    );
    let snapshot_version = snapshot.metadata().snapshot_version();

    match decision.result().action() {
        RouteAction::Block => {
            report(
                &dependencies.decisions,
                &observation,
                ObservedPath::Decision,
                None,
            );
            Ok(())
        }
        RouteAction::Direct => {
            let connected = connect_direct_udp(
                &target,
                exposed_remote.ip(),
                &dependencies.direct_domain_resolver,
                snapshot_version,
                Arc::clone(&dependencies.physical_interfaces),
            )
            .await;
            let path = match connected {
                Ok(value) => value,
                Err(error) => {
                    report(
                        &dependencies.decisions,
                        &observation,
                        ObservedPath::Decision,
                        Some("NP_WINDOWS_DIRECT_CONNECT_FAILED"),
                    );
                    return Err(error);
                }
            };
            let (socket, interface_index) = path.into_parts();
            report(
                &dependencies.decisions,
                &observation,
                ObservedPath::Direct {
                    interface_index,
                    fail_open: false,
                },
                None,
            );
            relay_direct(
                socket,
                first_payload,
                &mut incoming,
                dependencies.injector,
                injection,
            )
            .await
        }
        RouteAction::Proxy => {
            let outbound_id = decision
                .result()
                .outbound_id()
                .ok_or(GatewayError::InvalidContract("代理决策缺少出口"))?;
            let opened = async {
                let connector = load_connector_with_dialer(
                    &dependencies.gateway,
                    Arc::clone(&dependencies.credential_store),
                    outbound_id,
                    Arc::new(SystemTcpDialer),
                )
                .await
                .map_err(data_plane_error)?;
                connector.open_udp().await.map_err(data_plane_error)
            }
            .await;
            match opened {
                Ok(association) => {
                    report(
                        &dependencies.decisions,
                        &observation,
                        ObservedPath::Proxy {
                            outbound_id: outbound_id.clone(),
                        },
                        None,
                    );
                    relay_proxy(
                        association,
                        &target,
                        first_payload,
                        &mut incoming,
                        dependencies.injector,
                        injection,
                    )
                    .await
                }
                Err(_) if decision.result().failure_mode() == FailureMode::Open => {
                    let connected = connect_direct_udp(
                        &target,
                        exposed_remote.ip(),
                        &dependencies.direct_domain_resolver,
                        snapshot_version,
                        dependencies.physical_interfaces,
                    )
                    .await;
                    let path = match connected {
                        Ok(value) => value,
                        Err(error) => {
                            report(
                                &dependencies.decisions,
                                &observation,
                                ObservedPath::Decision,
                                Some("NP_WINDOWS_PROXY_FAIL_OPEN_FAILED"),
                            );
                            return Err(error);
                        }
                    };
                    let (socket, interface_index) = path.into_parts();
                    report(
                        &dependencies.decisions,
                        &observation,
                        ObservedPath::Direct {
                            interface_index,
                            fail_open: true,
                        },
                        Some("NP_WINDOWS_PROXY_FAIL_OPEN_DIRECT"),
                    );
                    relay_direct(
                        socket,
                        first_payload,
                        &mut incoming,
                        dependencies.injector,
                        injection,
                    )
                    .await
                }
                Err(error) => {
                    report(
                        &dependencies.decisions,
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

async fn session_target(
    datagram: &UdpDatagram,
    dependencies: &UdpSessionDependencies,
) -> Result<(FlowEndpoint, Destination), GatewayError> {
    let remote = datagram.remote();
    if dependencies
        .direct_domain_resolver
        .address_space()
        .contains(remote.ip())
    {
        let binding = dependencies
            .gateway
            .synthetic_dns_lookup(
                dependencies.direct_domain_resolver.address_space(),
                remote.ip(),
            )
            .await?
            .ok_or_else(|| {
                GatewayError::WindowsDataPlane("合成 DNS UDP 目标没有有效域名绑定".to_owned())
            })?;
        Ok((
            FlowEndpoint::Domain(binding.domain().clone(), remote.port()),
            Destination::new(
                Some(binding.domain().as_ascii()),
                None,
                remote.port(),
                Transport::Udp,
            )?,
        ))
    } else {
        Ok((
            FlowEndpoint::Ip(remote),
            Destination::new(None, Some(remote.ip()), remote.port(), Transport::Udp)?,
        ))
    }
}

async fn relay_direct(
    socket: UdpSocket,
    first: PendingUdpPayload,
    incoming: &mut mpsc::Receiver<PendingUdpPayload>,
    injector: UdpInjector,
    injection: UdpInjectionContext,
) -> Result<(), GatewayError> {
    send_direct(&socket, &first.bytes).await?;
    drop(first);
    let mut response = vec![0_u8; MAX_UDP_PAYLOAD_BYTES];
    loop {
        let event = timeout(SESSION_IDLE_TIMEOUT, async {
            tokio::select! {
                payload = incoming.recv() => RelayEvent::Payload(payload),
                received = socket.recv(&mut response) => RelayEvent::DirectResponse(received),
            }
        })
        .await;
        match event {
            Err(_) => return Ok(()),
            Ok(RelayEvent::Payload(Some(payload))) => {
                send_direct(&socket, &payload.bytes).await?;
            }
            Ok(RelayEvent::Payload(None)) => return Ok(()),
            Ok(RelayEvent::DirectResponse(Ok(length))) => {
                injector.inject(injection, &response[..length])?;
            }
            Ok(RelayEvent::DirectResponse(Err(error))) => {
                return Err(io_error("接收 Windows DIRECT UDP", error));
            }
            Ok(RelayEvent::ProxyResponse(_)) => {
                return Err(GatewayError::InvalidContract("直连 UDP relay 事件类型无效"));
            }
        }
    }
}

async fn relay_proxy(
    association: Socks5UdpAssociation,
    target: &FlowEndpoint,
    first: PendingUdpPayload,
    incoming: &mut mpsc::Receiver<PendingUdpPayload>,
    injector: UdpInjector,
    injection: UdpInjectionContext,
) -> Result<(), GatewayError> {
    association
        .send(target, &first.bytes)
        .await
        .map_err(data_plane_error)?;
    drop(first);
    loop {
        let event = timeout(SESSION_IDLE_TIMEOUT, async {
            tokio::select! {
                payload = incoming.recv() => RelayEvent::Payload(payload),
                received = association.receive() => RelayEvent::ProxyResponse(received),
            }
        })
        .await;
        match event {
            Err(_) => return Ok(()),
            Ok(RelayEvent::Payload(Some(payload))) => {
                association
                    .send(target, &payload.bytes)
                    .await
                    .map_err(data_plane_error)?;
            }
            Ok(RelayEvent::Payload(None)) => return Ok(()),
            Ok(RelayEvent::ProxyResponse(Ok((source, payload)))) => {
                if response_matches(target, &source) {
                    injector.inject(injection, &payload)?;
                }
            }
            Ok(RelayEvent::ProxyResponse(Err(error))) => return Err(data_plane_error(error)),
            Ok(RelayEvent::DirectResponse(_)) => {
                return Err(GatewayError::InvalidContract("代理 UDP relay 事件类型无效"));
            }
        }
    }
}

enum RelayEvent {
    Payload(Option<PendingUdpPayload>),
    DirectResponse(std::io::Result<usize>),
    ProxyResponse(Result<(FlowEndpoint, Vec<u8>), nonproxy_outbound::OutboundError>),
}

async fn send_direct(socket: &UdpSocket, payload: &[u8]) -> Result<(), GatewayError> {
    let sent = socket
        .send(payload)
        .await
        .map_err(|error| io_error("发送 Windows DIRECT UDP", error))?;
    if sent != payload.len() {
        return Err(GatewayError::WindowsDataPlane(
            "Windows DIRECT UDP 数据报未完整发送".to_owned(),
        ));
    }
    Ok(())
}

fn response_matches(target: &FlowEndpoint, source: &FlowEndpoint) -> bool {
    match (target, source) {
        (FlowEndpoint::Ip(expected), FlowEndpoint::Ip(actual)) => expected == actual,
        (FlowEndpoint::Domain(_, expected), FlowEndpoint::Domain(_, actual)) => expected == actual,
        (FlowEndpoint::Domain(_, expected), FlowEndpoint::Ip(actual)) => *expected == actual.port(),
        _ => false,
    }
}

fn io_error(operation: &'static str, source: std::io::Error) -> GatewayError {
    GatewayError::Io { operation, source }
}

fn data_plane_error(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::WindowsDataPlane(error.to_string())
}
