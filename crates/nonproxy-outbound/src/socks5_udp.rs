use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use nonproxy_flow_protocol::FlowEndpoint;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket, lookup_host},
    time::timeout,
};
use zeroize::Zeroizing;

use crate::{OutboundError, ProxyCredentials, ProxyEndpoint, TcpDialer};

const MAXIMUM_UDP_PACKET_BYTES: usize = 65_535;
const MAXIMUM_UDP_CONTENT_BYTES: usize = 65_000;

pub struct Socks5UdpAssociation {
    _control: TcpStream,
    socket: UdpSocket,
}

impl Socks5UdpAssociation {
    pub(crate) async fn open(
        proxy: &ProxyEndpoint,
        credentials: Option<&ProxyCredentials>,
        duration: Duration,
        dialer: Arc<dyn TcpDialer>,
    ) -> Result<Self, OutboundError> {
        let operation = async {
            let endpoint = FlowEndpoint::new(proxy.host(), proxy.port())
                .map_err(|_| OutboundError::InvalidEndpoint)?;
            let mut control = dialer.connect(&endpoint).await?;
            authenticate(&mut control, credentials).await?;
            let local = udp_local_endpoint(&control)?;
            let socket = UdpSocket::bind(local).await?;
            let mut request_endpoint = socket.local_addr()?;
            request_endpoint.set_ip(control.local_addr()?.ip());
            let request = command_request(3, &FlowEndpoint::Ip(request_endpoint))?;
            control.write_all(&request).await?;
            let relay = read_command_reply(&mut control).await?;
            let relay = resolve_relay(relay, control.peer_addr()?).await?;
            socket.connect(relay).await?;
            Ok(Self {
                _control: control,
                socket,
            })
        };
        timeout(duration, operation)
            .await
            .map_err(|_| OutboundError::ConnectTimeout)?
    }

    pub async fn send(&self, endpoint: &FlowEndpoint, content: &[u8]) -> Result<(), OutboundError> {
        if content.is_empty() || content.len() > MAXIMUM_UDP_CONTENT_BYTES {
            return Err(OutboundError::InvalidSocksResponse);
        }
        let mut packet = Vec::with_capacity(content.len() + 22);
        packet.extend_from_slice(&[0, 0, 0]);
        encode_endpoint(endpoint, &mut packet)?;
        packet.extend_from_slice(content);
        let sent = self.socket.send(&packet).await?;
        if sent != packet.len() {
            return Err(OutboundError::Io(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "SOCKS5 UDP 数据报未完整发送",
            )));
        }
        Ok(())
    }

    pub async fn receive(&self) -> Result<(FlowEndpoint, Vec<u8>), OutboundError> {
        let mut packet = vec![0_u8; MAXIMUM_UDP_PACKET_BYTES];
        let read = self.socket.recv(&mut packet).await?;
        packet.truncate(read);
        if packet.len() < 4 || packet[0..2] != [0, 0] {
            return Err(OutboundError::InvalidSocksResponse);
        }
        if packet[2] != 0 {
            return Err(OutboundError::SocksUdpFragmentUnsupported);
        }
        let (endpoint, consumed) = decode_endpoint(&packet[3..])?;
        let payload_start = 3 + consumed;
        if payload_start >= packet.len() {
            return Err(OutboundError::InvalidSocksResponse);
        }
        Ok((endpoint, packet[payload_start..].to_vec()))
    }
}

async fn authenticate(
    control: &mut TcpStream,
    credentials: Option<&ProxyCredentials>,
) -> Result<(), OutboundError> {
    let methods: &[u8] = if credentials.is_some() {
        &[5, 2, 0, 2]
    } else {
        &[5, 1, 0]
    };
    control.write_all(methods).await?;
    let mut response = [0_u8; 2];
    control.read_exact(&mut response).await?;
    if response[0] != 5 {
        return Err(OutboundError::InvalidSocksResponse);
    }
    match response[1] {
        0 => Ok(()),
        2 => {
            let credentials = credentials.ok_or(OutboundError::SocksAuthenticationFailed)?;
            authenticate_password(control, credentials).await
        }
        _ => Err(OutboundError::SocksAuthenticationFailed),
    }
}

async fn authenticate_password(
    control: &mut TcpStream,
    credentials: &ProxyCredentials,
) -> Result<(), OutboundError> {
    let username = credentials.username().as_bytes();
    let password = credentials.password().as_bytes();
    let username_length =
        u8::try_from(username.len()).map_err(|_| OutboundError::InvalidCredential)?;
    let password_length =
        u8::try_from(password.len()).map_err(|_| OutboundError::InvalidCredential)?;
    let mut request = Zeroizing::new(Vec::with_capacity(3 + username.len() + password.len()));
    request.extend_from_slice(&[1, username_length]);
    request.extend_from_slice(username);
    request.push(password_length);
    request.extend_from_slice(password);
    control.write_all(request.as_slice()).await?;
    let mut response = [0_u8; 2];
    control.read_exact(&mut response).await?;
    if response != [1, 0] {
        return Err(OutboundError::SocksAuthenticationFailed);
    }
    Ok(())
}

fn command_request(command: u8, endpoint: &FlowEndpoint) -> Result<Vec<u8>, OutboundError> {
    let mut request = Vec::with_capacity(22);
    request.extend_from_slice(&[5, command, 0]);
    encode_endpoint(endpoint, &mut request)?;
    Ok(request)
}

async fn read_command_reply(control: &mut TcpStream) -> Result<FlowEndpoint, OutboundError> {
    let mut header = [0_u8; 4];
    control.read_exact(&mut header).await?;
    if header[0] != 5 || header[1] != 0 || header[2] != 0 {
        return Err(OutboundError::InvalidSocksResponse);
    }
    read_endpoint(control, header[3]).await
}

async fn read_endpoint(control: &mut TcpStream, kind: u8) -> Result<FlowEndpoint, OutboundError> {
    let encoded = match kind {
        1 => read_variable(control, kind, 6).await?,
        3 => {
            let mut length = [0_u8; 1];
            control.read_exact(&mut length).await?;
            let mut value = vec![kind, length[0]];
            value.extend_from_slice(&read_bytes(control, usize::from(length[0]) + 2).await?);
            value
        }
        4 => read_variable(control, kind, 18).await?,
        _ => return Err(OutboundError::InvalidSocksResponse),
    };
    let (endpoint, consumed) = decode_endpoint(&encoded)?;
    if consumed != encoded.len() {
        return Err(OutboundError::InvalidSocksResponse);
    }
    Ok(endpoint)
}

async fn read_variable(
    control: &mut TcpStream,
    kind: u8,
    remaining: usize,
) -> Result<Vec<u8>, OutboundError> {
    let mut value = vec![kind];
    value.extend_from_slice(&read_bytes(control, remaining).await?);
    Ok(value)
}

async fn read_bytes(control: &mut TcpStream, length: usize) -> Result<Vec<u8>, OutboundError> {
    let mut value = vec![0_u8; length];
    control.read_exact(&mut value).await?;
    Ok(value)
}

fn encode_endpoint(endpoint: &FlowEndpoint, output: &mut Vec<u8>) -> Result<(), OutboundError> {
    match endpoint {
        FlowEndpoint::Ip(SocketAddr::V4(address)) => {
            output.push(1);
            output.extend_from_slice(&address.ip().octets());
            output.extend_from_slice(&address.port().to_be_bytes());
        }
        FlowEndpoint::Domain(domain, port) => {
            let bytes = domain.as_ascii().as_bytes();
            let length = u8::try_from(bytes.len()).map_err(|_| OutboundError::InvalidEndpoint)?;
            output.extend_from_slice(&[3, length]);
            output.extend_from_slice(bytes);
            output.extend_from_slice(&port.to_be_bytes());
        }
        FlowEndpoint::Ip(SocketAddr::V6(address)) => {
            output.push(4);
            output.extend_from_slice(&address.ip().octets());
            output.extend_from_slice(&address.port().to_be_bytes());
        }
    }
    Ok(())
}

fn decode_endpoint(input: &[u8]) -> Result<(FlowEndpoint, usize), OutboundError> {
    match input.first().copied() {
        Some(1) if input.len() >= 7 => {
            let ip = Ipv4Addr::new(input[1], input[2], input[3], input[4]);
            endpoint(IpAddr::V4(ip), input[5], input[6], 7)
        }
        Some(4) if input.len() >= 19 => {
            let octets: [u8; 16] = input[1..17]
                .try_into()
                .map_err(|_| OutboundError::InvalidSocksResponse)?;
            endpoint(IpAddr::V6(Ipv6Addr::from(octets)), input[17], input[18], 19)
        }
        Some(3) => decode_domain(input),
        _ => Err(OutboundError::InvalidSocksResponse),
    }
}

fn decode_domain(input: &[u8]) -> Result<(FlowEndpoint, usize), OutboundError> {
    let length = input
        .get(1)
        .copied()
        .map(usize::from)
        .ok_or(OutboundError::InvalidSocksResponse)?;
    let consumed = length
        .checked_add(4)
        .ok_or(OutboundError::InvalidSocksResponse)?;
    if length == 0 || input.len() < consumed {
        return Err(OutboundError::InvalidSocksResponse);
    }
    let host = std::str::from_utf8(&input[2..2 + length])
        .map_err(|_| OutboundError::InvalidSocksResponse)?;
    let port = u16::from_be_bytes([input[consumed - 2], input[consumed - 1]]);
    let endpoint =
        FlowEndpoint::new(host, port).map_err(|_| OutboundError::InvalidSocksResponse)?;
    Ok((endpoint, consumed))
}

fn endpoint(
    ip: IpAddr,
    port_high: u8,
    port_low: u8,
    consumed: usize,
) -> Result<(FlowEndpoint, usize), OutboundError> {
    let port = u16::from_be_bytes([port_high, port_low]);
    if port == 0 {
        return Err(OutboundError::InvalidSocksResponse);
    }
    Ok((FlowEndpoint::Ip(SocketAddr::new(ip, port)), consumed))
}

fn udp_local_endpoint(control: &TcpStream) -> Result<SocketAddr, OutboundError> {
    Ok(match control.local_addr()?.ip() {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    })
}

async fn resolve_relay(
    endpoint: FlowEndpoint,
    proxy_peer: SocketAddr,
) -> Result<SocketAddr, OutboundError> {
    match endpoint {
        FlowEndpoint::Ip(mut address) => {
            if address.ip().is_unspecified() {
                address.set_ip(proxy_peer.ip());
            }
            Ok(address)
        }
        FlowEndpoint::Domain(domain, port) => lookup_host((domain.as_ascii(), port))
            .await?
            .find(|address| address.is_ipv4() == proxy_peer.is_ipv4())
            .ok_or(OutboundError::InvalidSocksResponse),
    }
}
