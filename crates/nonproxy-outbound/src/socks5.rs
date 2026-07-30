use std::{sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use tokio::time::timeout;
use tokio_socks::tcp::Socks5Stream;

use crate::{
    BoxedProxyStream, OutboundError, ProxyCredentials, ProxyEndpoint, Socks5UdpAssociation,
    TcpDialer,
};

pub struct Socks5Connector {
    endpoint: ProxyEndpoint,
    credentials: Option<ProxyCredentials>,
    timeout: Duration,
    dialer: Arc<dyn TcpDialer>,
}

impl Socks5Connector {
    pub fn new(
        endpoint: ProxyEndpoint,
        credentials: Option<ProxyCredentials>,
        timeout: Duration,
        dialer: Arc<dyn TcpDialer>,
    ) -> Self {
        Self {
            endpoint,
            credentials,
            timeout,
            dialer,
        }
    }

    pub async fn connect(&self, target: &FlowEndpoint) -> Result<BoxedProxyStream, OutboundError> {
        let target = (target.host(), target.port());
        let operation = async {
            let proxy = FlowEndpoint::new(self.endpoint.host(), self.endpoint.port())
                .map_err(|_| OutboundError::InvalidEndpoint)?;
            let socket = self.dialer.connect(&proxy).await?;
            let stream = match self.credentials.as_ref() {
                Some(credentials) => {
                    Socks5Stream::connect_with_password_and_socket(
                        socket,
                        target,
                        credentials.username(),
                        credentials.password(),
                    )
                    .await?
                }
                None => Socks5Stream::connect_with_socket(socket, target).await?,
            };
            Ok::<BoxedProxyStream, OutboundError>(Box::new(stream))
        };
        timeout(self.timeout, operation)
            .await
            .map_err(|_| OutboundError::ConnectTimeout)?
    }

    pub async fn open_udp(&self) -> Result<Socks5UdpAssociation, OutboundError> {
        Socks5UdpAssociation::open(
            &self.endpoint,
            self.credentials.as_ref(),
            self.timeout,
            Arc::clone(&self.dialer),
        )
        .await
    }
}
