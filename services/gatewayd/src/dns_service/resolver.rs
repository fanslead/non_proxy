use std::{io, net::SocketAddr, num::NonZeroU32, time::Duration};

use nonproxy_dns::{ParsedDnsQuery, ParsedDnsResponse};
use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_outbound::OutboundConnector;
#[cfg(unix)]
use socket2::SockRef;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpSocket, UdpSocket},
    time::timeout,
};

use super::DnsServiceError;

const DNS_TIMEOUT: Duration = Duration::from_secs(5);
const MAXIMUM_DNS_MESSAGE_BYTES: usize = u16::MAX as usize;

pub struct ForwardedDnsResponse {
    pub bytes: Vec<u8>,
    pub resolver: SocketAddr,
}

pub async fn direct(
    upstreams: &[SocketAddr],
    parsed_query: &ParsedDnsQuery,
    query: &[u8],
    interface_index: Option<NonZeroU32>,
) -> Result<ForwardedDnsResponse, DnsServiceError> {
    for upstream in upstreams {
        let response = match udp_exchange(*upstream, query, interface_index).await {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !is_truncated(&response) && is_valid_response(parsed_query, &response) {
            return Ok(ForwardedDnsResponse {
                bytes: response,
                resolver: *upstream,
            });
        }
        if let Ok(response) = direct_tcp_exchange(*upstream, query, interface_index).await
            && is_valid_response(parsed_query, &response)
        {
            return Ok(ForwardedDnsResponse {
                bytes: response,
                resolver: *upstream,
            });
        }
    }
    Err(DnsServiceError::ResolversExhausted)
}

pub async fn proxy(
    connector: &OutboundConnector,
    upstreams: &[SocketAddr],
    parsed_query: &ParsedDnsQuery,
    query: &[u8],
) -> Result<ForwardedDnsResponse, DnsServiceError> {
    if connector.supports_udp()
        && let Ok(association) = connector.open_udp().await
    {
        for upstream in upstreams {
            let target = FlowEndpoint::Ip(*upstream);
            if association.send(&target, query).await.is_err() {
                continue;
            }
            let received = timeout(DNS_TIMEOUT, association.receive()).await;
            let Ok(Ok((source, response))) = received else {
                continue;
            };
            if source != target || !matches_query_id(query, &response) {
                continue;
            }
            if !is_truncated(&response) && is_valid_response(parsed_query, &response) {
                return Ok(ForwardedDnsResponse {
                    bytes: response,
                    resolver: *upstream,
                });
            }
            if let Ok(response) = proxy_tcp_exchange(connector, *upstream, query).await
                && is_valid_response(parsed_query, &response)
            {
                return Ok(ForwardedDnsResponse {
                    bytes: response,
                    resolver: *upstream,
                });
            }
        }
    }
    for upstream in upstreams {
        if let Ok(response) = proxy_tcp_exchange(connector, *upstream, query).await
            && is_valid_response(parsed_query, &response)
        {
            return Ok(ForwardedDnsResponse {
                bytes: response,
                resolver: *upstream,
            });
        }
    }
    Err(DnsServiceError::ResolversExhausted)
}

async fn udp_exchange(
    upstream: SocketAddr,
    query: &[u8],
    interface_index: Option<NonZeroU32>,
) -> Result<Vec<u8>, DnsServiceError> {
    let bind_address = if upstream.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let operation = async {
        let socket = UdpSocket::bind(bind_address).await?;
        if let Some(interface_index) = interface_index {
            bind_udp_interface(&socket, upstream, interface_index)?;
        }
        socket.connect(upstream).await?;
        let sent = socket.send(query).await?;
        if sent != query.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "DNS 查询未完整发送",
            ));
        }
        let mut response = vec![0_u8; MAXIMUM_DNS_MESSAGE_BYTES];
        let received = socket.recv(&mut response).await?;
        response.truncate(received);
        Ok::<Vec<u8>, io::Error>(response)
    };
    timeout(DNS_TIMEOUT, operation)
        .await
        .map_err(|_| DnsServiceError::ResolverTimeout)?
        .map_err(|_| DnsServiceError::ResolverIo)
}

async fn direct_tcp_exchange(
    upstream: SocketAddr,
    query: &[u8],
    interface_index: Option<NonZeroU32>,
) -> Result<Vec<u8>, DnsServiceError> {
    let operation = async {
        let socket = if upstream.is_ipv4() {
            TcpSocket::new_v4()?
        } else {
            TcpSocket::new_v6()?
        };
        if let Some(interface_index) = interface_index {
            bind_tcp_interface(&socket, upstream, interface_index)?;
        }
        let mut stream = socket.connect(upstream).await?;
        dns_tcp_exchange(&mut stream, query).await
    };
    timeout(DNS_TIMEOUT, operation)
        .await
        .map_err(|_| DnsServiceError::ResolverTimeout)?
        .map_err(|_| DnsServiceError::ResolverIo)
}

#[cfg(unix)]
fn bind_udp_interface(
    socket: &UdpSocket,
    upstream: SocketAddr,
    interface_index: NonZeroU32,
) -> io::Result<()> {
    bind_interface(&SockRef::from(socket), upstream, interface_index)
}

#[cfg(windows)]
fn bind_udp_interface(
    socket: &UdpSocket,
    upstream: SocketAddr,
    interface_index: NonZeroU32,
) -> io::Result<()> {
    nonproxy_windows_network::bind_unicast_interface(
        socket.as_raw_socket(),
        upstream.ip(),
        interface_index,
    )
    .map_err(io::Error::other)
}

#[cfg(all(not(unix), not(windows)))]
fn bind_udp_interface(
    _socket: &UdpSocket,
    _upstream: SocketAddr,
    _interface_index: NonZeroU32,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "当前平台尚未实现 DNS 物理网卡绑定",
    ))
}

#[cfg(unix)]
fn bind_tcp_interface(
    socket: &TcpSocket,
    upstream: SocketAddr,
    interface_index: NonZeroU32,
) -> io::Result<()> {
    bind_interface(&SockRef::from(socket), upstream, interface_index)
}

#[cfg(windows)]
fn bind_tcp_interface(
    socket: &TcpSocket,
    upstream: SocketAddr,
    interface_index: NonZeroU32,
) -> io::Result<()> {
    nonproxy_windows_network::bind_unicast_interface(
        socket.as_raw_socket(),
        upstream.ip(),
        interface_index,
    )
    .map_err(io::Error::other)
}

#[cfg(all(not(unix), not(windows)))]
fn bind_tcp_interface(
    _socket: &TcpSocket,
    _upstream: SocketAddr,
    _interface_index: NonZeroU32,
) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "当前平台尚未实现 DNS 物理网卡绑定",
    ))
}

#[cfg(unix)]
fn bind_interface(
    socket: &SockRef<'_>,
    upstream: SocketAddr,
    interface_index: NonZeroU32,
) -> io::Result<()> {
    if upstream.is_ipv4() {
        socket.bind_device_by_index_v4(Some(interface_index))
    } else {
        socket.bind_device_by_index_v6(Some(interface_index))
    }
}

async fn proxy_tcp_exchange(
    connector: &OutboundConnector,
    upstream: SocketAddr,
    query: &[u8],
) -> Result<Vec<u8>, DnsServiceError> {
    let target = FlowEndpoint::Ip(upstream);
    let mut stream = connector.connect_tcp(&target).await.map_err(|error| {
        DnsServiceError::Proxy(crate::flow_server::FlowServiceError::Outbound(error))
    })?;
    timeout(DNS_TIMEOUT, dns_tcp_exchange(&mut stream, query))
        .await
        .map_err(|_| DnsServiceError::ResolverTimeout)?
        .map_err(|_| DnsServiceError::ResolverIo)
}

async fn dns_tcp_exchange<TStream>(stream: &mut TStream, query: &[u8]) -> Result<Vec<u8>, io::Error>
where
    TStream: AsyncRead + AsyncWrite + Unpin,
{
    let query_length = u16::try_from(query.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "DNS 查询过长"))?;
    stream.write_all(&query_length.to_be_bytes()).await?;
    stream.write_all(query).await?;
    stream.flush().await?;
    let response_length = stream.read_u16().await?;
    if response_length == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DNS TCP 响应为空",
        ));
    }
    let mut response = vec![0_u8; usize::from(response_length)];
    stream.read_exact(&mut response).await?;
    Ok(response)
}

fn is_truncated(message: &[u8]) -> bool {
    message.get(2).is_some_and(|flags| flags & 0x02 != 0)
}

fn matches_query_id(query: &[u8], response: &[u8]) -> bool {
    query
        .get(..2)
        .zip(response.get(..2))
        .is_some_and(|(query_id, response_id)| query_id == response_id)
}

fn is_valid_response(query: &ParsedDnsQuery, response: &[u8]) -> bool {
    ParsedDnsResponse::parse(query, response).is_ok()
}
