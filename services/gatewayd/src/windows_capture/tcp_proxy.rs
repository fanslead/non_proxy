use std::{sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_model::{ConnectionContext, Destination, FailureMode, RouteAction, Transport};
use nonproxy_outbound::{OutboundError, TcpDialer};
use nonproxy_policy::PolicyEngine;
use nonproxy_windows_wfp::query_redirect_metadata;
use tokio::{
    io::{AsyncRead, AsyncWrite, copy_bidirectional},
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
    task::JoinSet,
    time::timeout,
};

use crate::{
    Gateway, GatewayError, credential_store::CredentialStore,
    flow_server::outbound_factory::load_connector_with_dialer,
};

use super::{
    dialer::RedirectTcpDialer,
    identity::{app_identity, original_remote},
    policy_cache::WindowsPolicyCache,
};

const MAXIMUM_ACTIVE_CONNECTIONS: usize = 2_048;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct WindowsTcpProxy {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    policies: WindowsPolicyCache,
    capacity: Arc<Semaphore>,
}

impl WindowsTcpProxy {
    pub fn new(
        gateway: Gateway,
        credential_store: Arc<dyn CredentialStore>,
        policies: WindowsPolicyCache,
    ) -> Self {
        Self {
            gateway,
            credential_store,
            policies,
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
        let gateway = self.gateway.clone();
        let credential_store = Arc::clone(&self.credential_store);
        let policies = self.policies.clone();
        tasks.spawn(async move {
            let _permit = permit;
            let _result = handle_connection(stream, gateway, credential_store, policies).await;
        });
    }
}

async fn handle_connection(
    mut inbound: TcpStream,
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    policies: WindowsPolicyCache,
) -> Result<(), GatewayError> {
    let metadata =
        query_redirect_metadata(std::os::windows::io::AsRawSocket::as_raw_socket(&inbound))
            .map_err(data_plane_error)?;
    let remote = original_remote(metadata.context())?;
    let target = FlowEndpoint::Ip(remote);
    let context = ConnectionContext::new(
        app_identity(metadata.context()),
        Destination::new(None, Some(remote.ip()), remote.port(), Transport::Tcp)?,
    );
    let snapshot = policies
        .current()
        .await
        .ok_or_else(|| GatewayError::WindowsDataPlane("没有可用的活动策略快照".to_owned()))?;
    let decision = PolicyEngine::decide(&snapshot, &context);
    let dialer: Arc<dyn TcpDialer> = Arc::new(RedirectTcpDialer::new(metadata.records()));
    match decision.result().action() {
        RouteAction::Block => Ok(()),
        RouteAction::Direct => {
            let mut outbound = connect_direct(Arc::clone(&dialer), &target).await?;
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
                    Arc::clone(&dialer),
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
                Ok(mut outbound) => relay(&mut inbound, &mut outbound).await,
                Err(_) if decision.result().failure_mode() == FailureMode::Open => {
                    let mut outbound = connect_direct(dialer, &target).await?;
                    relay(&mut inbound, &mut outbound).await
                }
                Err(error) => Err(error),
            }
        }
    }
}

async fn connect_direct(
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
