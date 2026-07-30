use std::{
    future::Future,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use nonproxy_dns::server_failure_response;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{Semaphore, watch},
    task::JoinSet,
    time::timeout,
};

use crate::{GatewayError, dns_service::DnsServiceError};

const MAXIMUM_ACTIVE_REQUESTS: usize = 1_024;
const MAXIMUM_UDP_QUERY_BYTES: usize = 4_096;
const MAXIMUM_TCP_QUERIES_PER_CONNECTION: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) type ProcessingFuture<'request> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, DnsServiceError>> + Send + 'request>>;

pub(crate) trait LocalDnsQueryProcessor: Send + Sync + 'static {
    fn process(&self, query: Vec<u8>) -> ProcessingFuture<'_>;
}

pub(crate) struct LocalDnsServer {
    udp_ipv4: Arc<UdpSocket>,
    udp_ipv6: Arc<UdpSocket>,
    tcp_ipv4: TcpListener,
    tcp_ipv6: TcpListener,
    #[cfg(test)]
    port: u16,
    capacity: Arc<Semaphore>,
}

impl LocalDnsServer {
    pub async fn bind_loopback(port: u16) -> Result<Self, GatewayError> {
        let tcp_ipv4 = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .await
            .map_err(|source| GatewayError::Io {
                operation: "绑定本地 DNS IPv4 TCP",
                source,
            })?;
        let port = tcp_ipv4
            .local_addr()
            .map_err(|source| GatewayError::Io {
                operation: "读取本地 DNS TCP 端口",
                source,
            })?
            .port();
        let udp_ipv4 = UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .await
            .map_err(|source| GatewayError::Io {
                operation: "绑定本地 DNS IPv4 UDP",
                source,
            })?;
        let tcp_ipv6 = TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))
            .await
            .map_err(|source| GatewayError::Io {
                operation: "绑定本地 DNS IPv6 TCP",
                source,
            })?;
        let udp_ipv6 = UdpSocket::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))
            .await
            .map_err(|source| GatewayError::Io {
                operation: "绑定本地 DNS IPv6 UDP",
                source,
            })?;
        Ok(Self {
            udp_ipv4: Arc::new(udp_ipv4),
            udp_ipv6: Arc::new(udp_ipv6),
            tcp_ipv4,
            tcp_ipv6,
            #[cfg(test)]
            port,
            capacity: Arc::new(Semaphore::new(MAXIMUM_ACTIVE_REQUESTS)),
        })
    }

    #[must_use]
    #[cfg(test)]
    pub const fn port(&self) -> u16 {
        self.port
    }

    pub async fn serve(
        self,
        processor: Arc<dyn LocalDnsQueryProcessor>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), GatewayError> {
        let mut udp_ipv4_buffer = vec![0_u8; MAXIMUM_UDP_QUERY_BYTES];
        let mut udp_ipv6_buffer = vec![0_u8; MAXIMUM_UDP_QUERY_BYTES];
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                received = self.udp_ipv4.recv_from(&mut udp_ipv4_buffer) => {
                    let (length, source) = received.map_err(|source| GatewayError::Io {
                        operation: "接收本地 DNS IPv4 UDP",
                        source,
                    })?;
                    spawn_udp(
                        Arc::clone(&self.udp_ipv4),
                        source,
                        udp_ipv4_buffer[..length].to_vec(),
                        Arc::clone(&processor),
                        Arc::clone(&self.capacity),
                        &mut tasks,
                    );
                }
                received = self.udp_ipv6.recv_from(&mut udp_ipv6_buffer) => {
                    let (length, source) = received.map_err(|source| GatewayError::Io {
                        operation: "接收本地 DNS IPv6 UDP",
                        source,
                    })?;
                    spawn_udp(
                        Arc::clone(&self.udp_ipv6),
                        source,
                        udp_ipv6_buffer[..length].to_vec(),
                        Arc::clone(&processor),
                        Arc::clone(&self.capacity),
                        &mut tasks,
                    );
                }
                accepted = self.tcp_ipv4.accept() => {
                    let (stream, _) = accepted.map_err(|source| GatewayError::Io {
                        operation: "接收本地 DNS IPv4 TCP",
                        source,
                    })?;
                    spawn_tcp(
                        stream,
                        Arc::clone(&processor),
                        Arc::clone(&self.capacity),
                        &mut tasks,
                    );
                }
                accepted = self.tcp_ipv6.accept() => {
                    let (stream, _) = accepted.map_err(|source| GatewayError::Io {
                        operation: "接收本地 DNS IPv6 TCP",
                        source,
                    })?;
                    spawn_tcp(
                        stream,
                        Arc::clone(&processor),
                        Arc::clone(&self.capacity),
                        &mut tasks,
                    );
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
}

fn spawn_udp(
    socket: Arc<UdpSocket>,
    source: SocketAddr,
    query: Vec<u8>,
    processor: Arc<dyn LocalDnsQueryProcessor>,
    capacity: Arc<Semaphore>,
    tasks: &mut JoinSet<()>,
) {
    let Ok(permit) = capacity.try_acquire_owned() else {
        return;
    };
    tasks.spawn(async move {
        let _permit = permit;
        let response = process_or_failure(processor.as_ref(), query).await;
        if let Some(response) = response {
            let _send_result = socket.send_to(&response, source).await;
        }
    });
}

fn spawn_tcp(
    stream: TcpStream,
    processor: Arc<dyn LocalDnsQueryProcessor>,
    capacity: Arc<Semaphore>,
    tasks: &mut JoinSet<()>,
) {
    let Ok(permit) = capacity.try_acquire_owned() else {
        return;
    };
    tasks.spawn(async move {
        let _permit = permit;
        let _result = serve_tcp_connection(stream, processor).await;
    });
}

async fn serve_tcp_connection(
    mut stream: TcpStream,
    processor: Arc<dyn LocalDnsQueryProcessor>,
) -> Result<(), std::io::Error> {
    for _query_count in 0..MAXIMUM_TCP_QUERIES_PER_CONNECTION {
        let length = match timeout(REQUEST_TIMEOUT, stream.read_u16()).await {
            Ok(Ok(value)) if value != 0 => usize::from(value),
            Ok(Ok(_)) | Ok(Err(_)) | Err(_) => return Ok(()),
        };
        let mut query = vec![0_u8; length];
        if !matches!(
            timeout(REQUEST_TIMEOUT, stream.read_exact(&mut query)).await,
            Ok(Ok(_))
        ) {
            return Ok(());
        }
        let Some(response) = process_or_failure(processor.as_ref(), query).await else {
            return Ok(());
        };
        let Ok(response_length) = u16::try_from(response.len()) else {
            return Ok(());
        };
        timeout(REQUEST_TIMEOUT, async {
            stream.write_all(&response_length.to_be_bytes()).await?;
            stream.write_all(&response).await?;
            stream.flush().await
        })
        .await
        .map_err(std::io::Error::other)??;
    }
    Ok(())
}

async fn process_or_failure(
    processor: &dyn LocalDnsQueryProcessor,
    query: Vec<u8>,
) -> Option<Vec<u8>> {
    match timeout(REQUEST_TIMEOUT, processor.process(query.clone())).await {
        Ok(Ok(response)) => Some(response),
        Ok(Err(_)) | Err(_) => server_failure_response(&query).ok(),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use hickory_proto::{
        op::{Message, MessageType, OpCode, Query},
        rr::{Name, RecordType},
    };
    use nonproxy_dns::synthetic_nodata_response;
    use tokio::{net::UdpSocket, sync::watch};

    use super::*;

    struct NoDataProcessor;

    impl LocalDnsQueryProcessor for NoDataProcessor {
        fn process(&self, query: Vec<u8>) -> ProcessingFuture<'_> {
            Box::pin(async move {
                synthetic_nodata_response(&query).map_err(DnsServiceError::InvalidQuery)
            })
        }
    }

    #[tokio::test]
    async fn udp_loopback_round_trip_preserves_transaction() -> Result<(), Box<dyn Error>> {
        let server = LocalDnsServer::bind_loopback(0).await?;
        let port = server.port();
        let (stop_sender, stop_receiver) = watch::channel(false);
        let task = tokio::spawn(server.serve(Arc::new(NoDataProcessor), stop_receiver));
        let client = UdpSocket::bind("127.0.0.1:0").await?;
        let mut query = Message::new(0xCAFE, MessageType::Query, OpCode::Query);
        query.add_query(Query::query(
            Name::from_ascii("listener.example.")?,
            RecordType::A,
        ));
        client
            .send_to(
                &query.to_vec()?,
                SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            )
            .await?;
        let mut response = [0_u8; 512];
        let received = timeout(Duration::from_secs(1), client.recv(&mut response)).await??;
        let parsed = Message::from_vec(&response[..received])?;

        assert_eq!(parsed.id, 0xCAFE);
        assert!(parsed.answers.is_empty());
        let _stop_result = stop_sender.send(true);
        assert!(task.await?.is_ok());
        Ok(())
    }
}
