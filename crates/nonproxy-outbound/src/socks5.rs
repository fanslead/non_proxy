use std::time::Duration;

use nonproxy_flow_protocol::FlowEndpoint;
use tokio::time::timeout;
use tokio_socks::tcp::Socks5Stream;

use crate::{
    BoxedProxyStream, OutboundError, ProxyCredentials, ProxyEndpoint, Socks5UdpAssociation,
};

pub struct Socks5Connector {
    endpoint: ProxyEndpoint,
    credentials: Option<ProxyCredentials>,
    timeout: Duration,
}

impl Socks5Connector {
    pub const fn new(
        endpoint: ProxyEndpoint,
        credentials: Option<ProxyCredentials>,
        timeout: Duration,
    ) -> Self {
        Self {
            endpoint,
            credentials,
            timeout,
        }
    }

    pub async fn connect(&self, target: &FlowEndpoint) -> Result<BoxedProxyStream, OutboundError> {
        let proxy = (self.endpoint.host(), self.endpoint.port());
        let target = (target.host(), target.port());
        let operation = async {
            let stream = match self.credentials.as_ref() {
                Some(credentials) => {
                    Socks5Stream::connect_with_password(
                        proxy,
                        target,
                        credentials.username(),
                        credentials.password(),
                    )
                    .await?
                }
                None => Socks5Stream::connect(proxy, target).await?,
            };
            Ok::<BoxedProxyStream, OutboundError>(Box::new(stream))
        };
        timeout(self.timeout, operation)
            .await
            .map_err(|_| OutboundError::ConnectTimeout)?
    }

    pub async fn open_udp(&self) -> Result<Socks5UdpAssociation, OutboundError> {
        Socks5UdpAssociation::open(&self.endpoint, self.credentials.as_ref(), self.timeout).await
    }
}
