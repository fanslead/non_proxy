use nonproxy_flow_protocol::FlowEndpoint;

use crate::{OutboundError, Socks5UdpAssociation, shadowsocks::ShadowsocksUdpAssociation};

pub enum OutboundDatagramSession {
    Socks5(Socks5UdpAssociation),
    Shadowsocks(ShadowsocksUdpAssociation),
}

impl OutboundDatagramSession {
    pub async fn send(&self, endpoint: &FlowEndpoint, content: &[u8]) -> Result<(), OutboundError> {
        match self {
            Self::Socks5(session) => session.send(endpoint, content).await,
            Self::Shadowsocks(session) => session.send(endpoint, content).await,
        }
    }

    pub async fn receive(&self) -> Result<(FlowEndpoint, Vec<u8>), OutboundError> {
        match self {
            Self::Socks5(session) => session.receive().await,
            Self::Shadowsocks(session) => session.receive().await,
        }
    }
}
