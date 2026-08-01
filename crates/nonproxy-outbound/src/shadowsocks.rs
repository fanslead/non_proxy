use std::{sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use shadowsocks::{
    ProxyClientStream, ServerConfig,
    config::ServerType,
    context::{Context, SharedContext},
    net::UdpSocket as ShadowUdpSocket,
    relay::{socks5::Address, udprelay::ProxySocket},
};
use tokio::{io::AsyncWriteExt, time::timeout};

use crate::{BoxedProxyStream, OutboundError, ProxyEndpoint, ShadowsocksCredentials, TcpDialer};

const MAXIMUM_UDP_PACKET_BYTES: usize = 65_535;

pub struct ShadowsocksConnector {
    endpoint: ProxyEndpoint,
    credentials: ShadowsocksCredentials,
    timeout: Duration,
    dialer: Arc<dyn TcpDialer>,
    context: SharedContext,
}

impl ShadowsocksConnector {
    pub fn new(
        endpoint: ProxyEndpoint,
        credentials: ShadowsocksCredentials,
        timeout: Duration,
        dialer: Arc<dyn TcpDialer>,
    ) -> Self {
        Self {
            endpoint,
            credentials,
            timeout,
            dialer,
            context: Context::new_shared(ServerType::Local),
        }
    }

    pub async fn connect(&self, target: &FlowEndpoint) -> Result<BoxedProxyStream, OutboundError> {
        let operation = async {
            let proxy = FlowEndpoint::new(self.endpoint.host(), self.endpoint.port())
                .map_err(|_| OutboundError::InvalidEndpoint)?;
            let stream = self.dialer.connect(&proxy).await?;
            let config = self.server_config()?;
            let mut stream = ProxyClientStream::from_stream(
                Arc::clone(&self.context),
                stream,
                &config,
                shadowsocks_address(target),
            );
            let sent = stream.write(&[]).await?;
            if sent != 0 {
                return Err(OutboundError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Shadowsocks 空握手写入返回了非零长度",
                )));
            }
            stream.flush().await?;
            Ok::<BoxedProxyStream, OutboundError>(Box::new(stream))
        };
        timeout(self.timeout, operation)
            .await
            .map_err(|_| OutboundError::ConnectTimeout)?
    }

    pub async fn open_udp(&self) -> Result<ShadowsocksUdpAssociation, OutboundError> {
        let config = self.server_config()?;
        let socket = timeout(
            self.timeout,
            ProxySocket::connect(Arc::clone(&self.context), &config),
        )
        .await
        .map_err(|_| OutboundError::ConnectTimeout)?
        .map_err(|_| OutboundError::ShadowsocksDatagram)?;
        Ok(ShadowsocksUdpAssociation { socket })
    }

    fn server_config(&self) -> Result<ServerConfig, OutboundError> {
        let mut config = self
            .credentials
            .server_config(self.endpoint.host(), self.endpoint.port())?;
        config.set_timeout(self.timeout);
        Ok(config)
    }
}

pub struct ShadowsocksUdpAssociation {
    socket: ProxySocket<ShadowUdpSocket>,
}

impl ShadowsocksUdpAssociation {
    pub async fn send(&self, endpoint: &FlowEndpoint, content: &[u8]) -> Result<(), OutboundError> {
        self.socket
            .send(&shadowsocks_address(endpoint), content)
            .await
            .map(|_| ())
            .map_err(|_| OutboundError::ShadowsocksDatagram)
    }

    pub async fn receive(&self) -> Result<(FlowEndpoint, Vec<u8>), OutboundError> {
        let mut packet = vec![0_u8; MAXIMUM_UDP_PACKET_BYTES];
        let (payload_length, endpoint, _) = self
            .socket
            .recv(&mut packet)
            .await
            .map_err(|_| OutboundError::ShadowsocksDatagram)?;
        packet.truncate(payload_length);
        Ok((flow_endpoint(endpoint)?, packet))
    }
}

fn shadowsocks_address(endpoint: &FlowEndpoint) -> Address {
    match endpoint {
        FlowEndpoint::Ip(address) => Address::SocketAddress(*address),
        FlowEndpoint::Domain(domain, port) => {
            Address::DomainNameAddress(domain.as_ascii().to_owned(), *port)
        }
    }
}

fn flow_endpoint(endpoint: Address) -> Result<FlowEndpoint, OutboundError> {
    match endpoint {
        Address::SocketAddress(address) => Ok(FlowEndpoint::Ip(address)),
        Address::DomainNameAddress(domain, port) => {
            FlowEndpoint::new(&domain, port).map_err(|_| OutboundError::ShadowsocksDatagram)
        }
    }
}
