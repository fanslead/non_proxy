mod activation;
mod dialer;
mod direct_dns;
mod dns_decision;
mod dns_proxy;
mod dns_runtime;
mod identity;
mod policy_cache;
mod tcp_proxy;
mod udp_direct;
mod udp_driver;
mod udp_proxy;
mod udp_session;

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use nonproxy_windows_identity::WindowsAppIdentityResolver;
use nonproxy_windows_network::PhysicalInterfaceCatalog;
use nonproxy_windows_wfp::{DynamicWfpSession, WfpConfig, WfpDriver};
use tokio::{net::TcpListener, sync::watch};

use crate::{
    Gateway, GatewayError,
    clock::unix_time_ms,
    credential_store::CredentialStore,
    decision_event::{DecisionEventWorker, decision_event_channel},
};

use activation::{WfpActivation, WfpRedirectPorts};
use dns_proxy::WindowsDnsProxy;
use policy_cache::WindowsPolicyCache;
use tcp_proxy::WindowsTcpProxy;
use udp_proxy::WindowsUdpProxy;
use udp_session::UdpSessionDependencies;

pub struct WindowsCapture {
    _bfe: DynamicWfpSession,
    activation: WfpActivation,
    dns: WindowsDnsProxy,
    proxy: WindowsTcpProxy,
    udp: WindowsUdpProxy,
    ipv4: TcpListener,
    ipv6: TcpListener,
    policies: WindowsPolicyCache,
    decisions: DecisionEventWorker,
    shutdown: watch::Receiver<bool>,
}

impl WindowsCapture {
    pub async fn start(
        gateway: Gateway,
        credential_store: Arc<dyn CredentialStore>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<Self, GatewayError> {
        let ipv4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(|source| GatewayError::Io {
                operation: "绑定 Windows IPv4 重定向监听",
                source,
            })?;
        let ipv6 = TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, 0)))
            .await
            .map_err(|source| GatewayError::Io {
                operation: "绑定 Windows IPv6 重定向监听",
                source,
            })?;
        let ipv4_port = local_port(&ipv4, "读取 Windows IPv4 重定向端口")?;
        let ipv6_port = local_port(&ipv6, "读取 Windows IPv6 重定向端口")?;
        let policies = WindowsPolicyCache::load(gateway.clone(), "windows-wfp", true).await?;
        let (decisions, decision_worker) = decision_event_channel(gateway.clone());
        let physical_interfaces = Arc::new(PhysicalInterfaceCatalog::new());
        let application_identities = Arc::new(WindowsAppIdentityResolver::new());
        let (dns, dns_ready, direct_domain_resolver) = WindowsDnsProxy::start(
            gateway.clone(),
            Arc::clone(&credential_store),
            Arc::clone(&physical_interfaces),
            decisions.clone(),
        )
        .await?;
        let (ipv4_dns_port, ipv6_dns_port) = dns.redirect_ports();
        let generation = unix_time_ms()?;
        let driver = Arc::new(WfpDriver::open().map_err(data_plane_error)?);
        driver
            .apply(&WfpConfig::disabled(generation))
            .map_err(data_plane_error)?;
        let bfe = DynamicWfpSession::install().map_err(data_plane_error)?;
        let process_id = u64::from(std::process::id());
        let generation = generation
            .checked_add(1)
            .ok_or_else(|| GatewayError::WindowsDataPlane("WFP 配置代次耗尽".to_owned()))?;
        driver
            .apply(&WfpConfig::dns_only(
                generation,
                process_id,
                ipv4_dns_port.to_be(),
                ipv6_dns_port.to_be(),
            ))
            .map_err(data_plane_error)?;
        let udp = WindowsUdpProxy::start(Arc::clone(&driver), |injector| UdpSessionDependencies {
            gateway: gateway.clone(),
            credential_store: Arc::clone(&credential_store),
            policies: policies.clone(),
            physical_interfaces: Arc::clone(&physical_interfaces),
            direct_domain_resolver: direct_domain_resolver.clone(),
            injector,
            decisions: decisions.clone(),
            application_identities: Arc::clone(&application_identities),
        });
        let proxy = WindowsTcpProxy::new(
            gateway,
            credential_store,
            policies.clone(),
            physical_interfaces,
            direct_domain_resolver,
            decisions,
            application_identities,
        );
        let activation = WfpActivation::new(
            driver,
            policies.clone(),
            generation,
            process_id,
            WfpRedirectPorts {
                tcp_ipv4: ipv4_port.to_be(),
                tcp_ipv6: ipv6_port.to_be(),
                dns_ipv4: ipv4_dns_port.to_be(),
                dns_ipv6: ipv6_dns_port.to_be(),
            },
            dns_ready,
        );
        Ok(Self {
            _bfe: bfe,
            activation,
            dns,
            proxy,
            udp,
            ipv4,
            ipv6,
            policies,
            decisions: decision_worker,
            shutdown,
        })
    }

    pub async fn serve(self) -> Result<(), GatewayError> {
        let Self {
            _bfe,
            activation,
            dns,
            proxy,
            udp,
            ipv4,
            ipv6,
            policies,
            decisions,
            mut shutdown,
        } = self;
        let (worker_stop_sender, worker_stop_receiver) = watch::channel(false);
        let (activation_stop_sender, activation_stop_receiver) = watch::channel(false);
        let proxy_server = proxy.serve(ipv4, ipv6, worker_stop_receiver.clone());
        let policy_server = policies.refresh_until_shutdown(worker_stop_receiver.clone());
        let dns_server = dns.serve(worker_stop_receiver.clone());
        let udp_server = udp.serve(worker_stop_receiver.clone());
        let decision_server = decisions.serve(worker_stop_receiver.clone());
        let activation_server = activation.serve(activation_stop_receiver);
        let worker_server = async {
            tokio::try_join!(
                proxy_server,
                policy_server,
                dns_server,
                udp_server,
                decision_server
            )
            .map(|_| ())
        };
        tokio::pin!(worker_server);
        tokio::pin!(activation_server);
        let first = tokio::select! {
            changed = shutdown.changed() => {
                let _changed = changed;
                FirstCompletion::Shutdown
            }
            result = &mut worker_server => FirstCompletion::Workers(result),
            result = &mut activation_server => FirstCompletion::Activation(result),
        };
        match first {
            FirstCompletion::Shutdown => {
                let _stop_result = activation_stop_sender.send(true);
                let _stop_result = worker_stop_sender.send(true);
                let (activation_result, worker_result) =
                    tokio::join!(&mut activation_server, &mut worker_server);
                activation_result?;
                worker_result
            }
            FirstCompletion::Workers(worker_result) => {
                let _stop_result = activation_stop_sender.send(true);
                let activation_result = activation_server.await;
                activation_result?;
                worker_result
            }
            FirstCompletion::Activation(activation_result) => {
                let _stop_result = worker_stop_sender.send(true);
                let worker_result = worker_server.await;
                activation_result?;
                worker_result
            }
        }
    }
}

enum FirstCompletion {
    Shutdown,
    Workers(Result<(), GatewayError>),
    Activation(Result<(), GatewayError>),
}

fn local_port(listener: &TcpListener, operation: &'static str) -> Result<u16, GatewayError> {
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|source| GatewayError::Io { operation, source })
}

fn data_plane_error(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::WindowsDataPlane(error.to_string())
}
