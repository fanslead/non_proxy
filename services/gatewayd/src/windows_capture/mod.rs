mod activation;
mod dialer;
mod identity;
mod policy_cache;
mod tcp_proxy;

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use nonproxy_windows_network::PhysicalInterfaceCatalog;
use nonproxy_windows_wfp::{DynamicWfpSession, WfpConfig, WfpDriver};
use tokio::{net::TcpListener, sync::watch};

use crate::{Gateway, GatewayError, clock::unix_time_ms, credential_store::CredentialStore};

use activation::WfpActivation;
use policy_cache::WindowsPolicyCache;
use tcp_proxy::WindowsTcpProxy;

pub struct WindowsCapture {
    _bfe: DynamicWfpSession,
    activation: WfpActivation,
    proxy: WindowsTcpProxy,
    ipv4: TcpListener,
    ipv6: TcpListener,
    policies: WindowsPolicyCache,
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
        let policies = WindowsPolicyCache::load(gateway.clone()).await?;
        let physical_interfaces = Arc::new(PhysicalInterfaceCatalog::new());
        let proxy = WindowsTcpProxy::new(
            gateway,
            credential_store,
            policies.clone(),
            physical_interfaces,
        );
        let generation = unix_time_ms()?;
        let driver = WfpDriver::open().map_err(data_plane_error)?;
        driver
            .apply(&WfpConfig::disabled(generation))
            .map_err(data_plane_error)?;
        let bfe = DynamicWfpSession::install().map_err(data_plane_error)?;
        let process_id = u64::from(std::process::id());
        let activation = WfpActivation::new(
            driver,
            policies.clone(),
            generation,
            process_id,
            ipv4_port.to_be(),
            ipv6_port.to_be(),
        );
        Ok(Self {
            _bfe: bfe,
            activation,
            proxy,
            ipv4,
            ipv6,
            policies,
            shutdown,
        })
    }

    pub async fn serve(self) -> Result<(), GatewayError> {
        let Self {
            _bfe,
            activation,
            proxy,
            ipv4,
            ipv6,
            policies,
            mut shutdown,
        } = self;
        let (worker_stop_sender, worker_stop_receiver) = watch::channel(false);
        let (activation_stop_sender, activation_stop_receiver) = watch::channel(false);
        let proxy_server = proxy.serve(ipv4, ipv6, worker_stop_receiver.clone());
        let policy_server = policies.refresh_until_shutdown(worker_stop_receiver);
        let activation_server = activation.serve(activation_stop_receiver);
        tokio::pin!(proxy_server);
        tokio::pin!(policy_server);
        tokio::pin!(activation_server);
        let first = tokio::select! {
            changed = shutdown.changed() => {
                let _changed = changed;
                FirstCompletion::Shutdown
            }
            result = &mut proxy_server => FirstCompletion::Proxy(result),
            result = &mut policy_server => FirstCompletion::Policy(result),
            result = &mut activation_server => FirstCompletion::Activation(result),
        };
        match first {
            FirstCompletion::Shutdown => {
                let _stop_result = activation_stop_sender.send(true);
                let activation_result = activation_server.await;
                let _stop_result = worker_stop_sender.send(true);
                let (proxy_result, policy_result) =
                    tokio::join!(&mut proxy_server, &mut policy_server);
                activation_result?;
                proxy_result?;
                policy_result
            }
            FirstCompletion::Proxy(proxy_result) => {
                let _stop_result = activation_stop_sender.send(true);
                let activation_result = activation_server.await;
                let _stop_result = worker_stop_sender.send(true);
                let policy_result = policy_server.await;
                activation_result?;
                proxy_result?;
                policy_result
            }
            FirstCompletion::Policy(policy_result) => {
                let _stop_result = activation_stop_sender.send(true);
                let activation_result = activation_server.await;
                let _stop_result = worker_stop_sender.send(true);
                let proxy_result = proxy_server.await;
                activation_result?;
                policy_result?;
                proxy_result
            }
            FirstCompletion::Activation(activation_result) => {
                let _stop_result = worker_stop_sender.send(true);
                let (proxy_result, policy_result) =
                    tokio::join!(&mut proxy_server, &mut policy_server);
                activation_result?;
                proxy_result?;
                policy_result
            }
        }
    }
}

enum FirstCompletion {
    Shutdown,
    Proxy(Result<(), GatewayError>),
    Policy(Result<(), GatewayError>),
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
