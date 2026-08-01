use std::{sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    OutboundDatagramSession, OutboundError, ProxyCredentials, ProxyEndpoint,
    ShadowsocksCredentials, SystemTcpDialer, TcpDialer, http_connect::HttpConnectConnector,
    shadowsocks::ShadowsocksConnector, socks5::Socks5Connector,
};

pub trait ProxyStream: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T> ProxyStream for T where T: AsyncRead + AsyncWrite + Send + Unpin {}

pub type BoxedProxyStream = Box<dyn ProxyStream>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectorKind {
    Socks5,
    HttpConnect,
}

pub enum OutboundConnector {
    Socks5(Socks5Connector),
    HttpConnect(HttpConnectConnector),
    Shadowsocks(ShadowsocksConnector),
}

impl OutboundConnector {
    pub fn new(
        kind: ConnectorKind,
        endpoint: ProxyEndpoint,
        credentials: Option<ProxyCredentials>,
        timeout: Duration,
    ) -> Self {
        Self::with_dialer(
            kind,
            endpoint,
            credentials,
            timeout,
            Arc::new(SystemTcpDialer),
        )
    }

    pub fn with_dialer(
        kind: ConnectorKind,
        endpoint: ProxyEndpoint,
        credentials: Option<ProxyCredentials>,
        timeout: Duration,
        dialer: Arc<dyn TcpDialer>,
    ) -> Self {
        match kind {
            ConnectorKind::Socks5 => {
                Self::Socks5(Socks5Connector::new(endpoint, credentials, timeout, dialer))
            }
            ConnectorKind::HttpConnect => Self::HttpConnect(HttpConnectConnector::new(
                endpoint,
                credentials,
                timeout,
                dialer,
            )),
        }
    }

    pub fn shadowsocks(
        endpoint: ProxyEndpoint,
        credentials: ShadowsocksCredentials,
        timeout: Duration,
    ) -> Self {
        Self::shadowsocks_with_dialer(endpoint, credentials, timeout, Arc::new(SystemTcpDialer))
    }

    pub fn shadowsocks_with_dialer(
        endpoint: ProxyEndpoint,
        credentials: ShadowsocksCredentials,
        timeout: Duration,
        dialer: Arc<dyn TcpDialer>,
    ) -> Self {
        Self::Shadowsocks(ShadowsocksConnector::new(
            endpoint,
            credentials,
            timeout,
            dialer,
        ))
    }

    pub async fn connect_tcp(
        &self,
        target: &FlowEndpoint,
    ) -> Result<BoxedProxyStream, OutboundError> {
        match self {
            Self::Socks5(connector) => connector.connect(target).await,
            Self::HttpConnect(connector) => connector.connect(target).await,
            Self::Shadowsocks(connector) => connector.connect(target).await,
        }
    }

    pub async fn open_udp(&self) -> Result<OutboundDatagramSession, OutboundError> {
        match self {
            Self::Socks5(connector) => connector
                .open_udp()
                .await
                .map(OutboundDatagramSession::Socks5),
            Self::HttpConnect(_) => Err(OutboundError::UdpUnsupported),
            Self::Shadowsocks(connector) => connector
                .open_udp()
                .await
                .map(OutboundDatagramSession::Shadowsocks),
        }
    }

    #[must_use]
    pub const fn supports_udp(&self) -> bool {
        matches!(self, Self::Socks5(_) | Self::Shadowsocks(_))
    }

    #[must_use]
    pub const fn requires_authenticated_tls_probe(&self) -> bool {
        matches!(self, Self::Shadowsocks(_))
    }
}
