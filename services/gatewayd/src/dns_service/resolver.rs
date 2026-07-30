use std::{io, net::SocketAddr, time::Duration};

use nonproxy_dns::{ParsedDnsQuery, ParsedDnsResponse};
use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_outbound::OutboundConnector;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
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
) -> Result<ForwardedDnsResponse, DnsServiceError> {
    for upstream in upstreams {
        let response = match udp_exchange(*upstream, query).await {
            Ok(value) => value,
            Err(_) => continue,
        };
        if !is_truncated(&response) && is_valid_response(parsed_query, &response) {
            return Ok(ForwardedDnsResponse {
                bytes: response,
                resolver: *upstream,
            });
        }
        if let Ok(response) = direct_tcp_exchange(*upstream, query).await
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

async fn udp_exchange(upstream: SocketAddr, query: &[u8]) -> Result<Vec<u8>, DnsServiceError> {
    let bind_address = if upstream.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let operation = async {
        let socket = UdpSocket::bind(bind_address).await?;
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
) -> Result<Vec<u8>, DnsServiceError> {
    let operation = async {
        let mut stream = TcpStream::connect(upstream).await?;
        dns_tcp_exchange(&mut stream, query).await
    };
    timeout(DNS_TIMEOUT, operation)
        .await
        .map_err(|_| DnsServiceError::ResolverTimeout)?
        .map_err(|_| DnsServiceError::ResolverIo)
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
