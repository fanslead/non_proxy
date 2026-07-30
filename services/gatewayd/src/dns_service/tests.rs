use std::{error::Error, net::SocketAddr, sync::Arc};

use hickory_proto::{
    op::{Message, MessageType, OpCode, Query},
    rr::{Name, RData, Record, RecordType, rdata::A},
};
use nonproxy_dns::ParsedDnsQuery;
use nonproxy_outbound::{ConnectorKind, OutboundConnector, ProxyEndpoint};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1::{AppIdentity, Platform},
    provider::v1::{DnsRouteKind, DnsUpstreamEndpoint, ResolveDnsRequest},
};
use nonproxy_storage::{PolicyDatabase, ProviderAck};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};

use super::{DnsResolutionService, request::ValidatedDnsRequest, resolver};
use crate::{
    Gateway,
    clock::unix_time_ms,
    credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
};

fn query_bytes(id: u16, qname: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut message = Message::new(id, MessageType::Query, OpCode::Query);
    message.add_query(Query::query(Name::from_ascii(qname)?, RecordType::A));
    Ok(message.to_vec()?)
}

fn response_bytes(query: &[u8], ttl: u32, truncated: bool) -> Result<Vec<u8>, Box<dyn Error>> {
    let query = Message::from_vec(query)?;
    let question = query.queries.first().ok_or("测试查询缺少 question")?;
    let mut response = Message::response(query.id, OpCode::Query);
    response.metadata.truncation = truncated;
    response.add_query(question.clone());
    if !truncated {
        response.add_answer(Record::from_rdata(
            question.name().clone(),
            ttl,
            RData::A(A::new(198, 51, 100, 9)),
        ));
    }
    Ok(response.to_vec()?)
}

fn request(query: Vec<u8>, upstream: SocketAddr, route: DnsRouteKind) -> ResolveDnsRequest {
    ResolveDnsRequest {
        context: None,
        query_id: "query-1".to_owned(),
        app: Some(AppIdentity {
            platform: Platform::Macos as i32,
            stable_id: "com.example.browser".to_owned(),
            ..Default::default()
        }),
        qname: "dns.example".to_owned(),
        qtype: u32::from(u16::from(RecordType::A)),
        network_profile_id: "office".to_owned(),
        dns_message: query,
        requested_route: route as i32,
        requested_outbound_id: String::new(),
        upstreams: vec![DnsUpstreamEndpoint {
            ip_address: upstream.ip().to_string(),
            port: u32::from(upstream.port()),
            scope_id: 0,
        }],
        snapshot_version: 1,
    }
}

async fn service() -> Result<DnsResolutionService, Box<dyn Error>> {
    let database = PolicyDatabase::open_in_memory(1)?;
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let published = gateway.compile_and_stage().await?;
    let content_hash = *published.artifact().content_hash();
    let required = vec!["transparent-proxy".to_owned(), "dns-proxy".to_owned()];
    gateway
        .acknowledge_provider_snapshot(
            1,
            ProviderAck::loaded("transparent-proxy", 1, content_hash, unix_time_ms()?)?,
            required.clone(),
        )
        .await?;
    gateway
        .acknowledge_provider_snapshot(
            1,
            ProviderAck::loaded("dns-proxy", 1, content_hash, unix_time_ms()?)?,
            required,
        )
        .await?;
    let credentials: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    Ok(DnsResolutionService::new(gateway, credentials))
}

#[tokio::test]
async fn direct_resolution_is_validated_and_cached() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let upstream = socket.local_addr()?;
    let fixture = tokio::spawn(async move {
        let mut query = vec![0_u8; u16::MAX as usize];
        let (received, peer) = socket.recv_from(&mut query).await?;
        query.truncate(received);
        let response = response_bytes(&query, 60, false)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        socket.send_to(&response, peer).await?;
        Ok::<(), std::io::Error>(())
    });
    let query = query_bytes(0x1234, "dns.example.")?;
    let service = service().await?;
    let first = service
        .resolve(request(query.clone(), upstream, DnsRouteKind::Direct))
        .await?;
    fixture.await??;
    assert!(!first.cache_hit);
    assert_eq!(first.valid_for_seconds, 60);
    assert_eq!(first.resolver_endpoint, Some(upstream.to_string()));

    let second = service
        .resolve(request(query, upstream, DnsRouteKind::Direct))
        .await?;
    assert!(second.cache_hit);
    assert_eq!(second.resolver_endpoint, None);
    assert_eq!(Message::from_vec(&second.dns_message)?.id, 0x1234);
    Ok(())
}

#[tokio::test]
async fn direct_udp_truncation_falls_back_to_tcp() -> Result<(), Box<dyn Error>> {
    let tcp = TcpListener::bind("127.0.0.1:0").await?;
    let upstream = tcp.local_addr()?;
    let udp = UdpSocket::bind(upstream).await?;
    let udp_fixture = tokio::spawn(async move {
        let mut query = vec![0_u8; u16::MAX as usize];
        let (received, peer) = udp.recv_from(&mut query).await?;
        query.truncate(received);
        let response = response_bytes(&query, 60, true)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        udp.send_to(&response, peer).await?;
        Ok::<(), std::io::Error>(())
    });
    let tcp_fixture = tokio::spawn(async move {
        let (mut stream, _) = tcp.accept().await?;
        let length = stream.read_u16().await?;
        let mut query = vec![0_u8; usize::from(length)];
        stream.read_exact(&mut query).await?;
        let response = response_bytes(&query, 45, false)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        stream
            .write_all(
                &u16::try_from(response.len())
                    .map_err(std::io::Error::other)?
                    .to_be_bytes(),
            )
            .await?;
        stream.write_all(&response).await?;
        Ok::<(), std::io::Error>(())
    });

    let query = query_bytes(0x2233, "dns.example.")?;
    let parsed_query = ParsedDnsQuery::parse(&query)?;
    let response = resolver::direct(&[upstream], &parsed_query, &query).await?;
    udp_fixture.await??;
    tcp_fixture.await??;
    assert_eq!(response.resolver, upstream);
    let message = Message::from_vec(&response.bytes)?;
    assert!(!message.truncation);
    assert_eq!(message.answers[0].ttl, 45);
    Ok(())
}

#[tokio::test]
async fn http_connect_proxy_uses_dns_tcp_framing() -> Result<(), Box<dyn Error>> {
    let proxy = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_address = proxy.local_addr()?;
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = proxy.accept().await?;
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let byte = stream.read_u8().await?;
            header.push(byte);
            if header.len() > 16 * 1024 {
                return Err(std::io::Error::other("CONNECT header 过长"));
            }
        }
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        let length = stream.read_u16().await?;
        let mut query = vec![0_u8; usize::from(length)];
        stream.read_exact(&mut query).await?;
        let response = response_bytes(&query, 30, false)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        stream
            .write_all(
                &u16::try_from(response.len())
                    .map_err(std::io::Error::other)?
                    .to_be_bytes(),
            )
            .await?;
        stream.write_all(&response).await?;
        Ok::<(), std::io::Error>(())
    });
    let endpoint = ProxyEndpoint::new(proxy_address.ip().to_string(), proxy_address.port())?;
    let connector = OutboundConnector::new(
        ConnectorKind::HttpConnect,
        endpoint,
        None,
        std::time::Duration::from_secs(2),
    );
    let query = query_bytes(0x3344, "dns.example.")?;
    let upstream = "203.0.113.53:53".parse::<SocketAddr>()?;
    let parsed_query = ParsedDnsQuery::parse(&query)?;
    let response = resolver::proxy(&connector, &[upstream], &parsed_query, &query).await?;
    fixture.await??;
    assert_eq!(response.resolver, upstream);
    assert_eq!(Message::from_vec(&response.bytes)?.answers[0].ttl, 30);
    Ok(())
}

#[test]
fn request_rejects_metadata_that_disagrees_with_wire_query() -> Result<(), Box<dyn Error>> {
    let upstream = "127.0.0.1:53".parse::<SocketAddr>()?;
    let query = query_bytes(0x4455, "other.example.")?;
    let error = ValidatedDnsRequest::parse(request(query, upstream, DnsRouteKind::Direct));
    assert!(error.is_err());
    Ok(())
}

#[test]
fn request_preserves_ipv6_resolver_scope() -> Result<(), Box<dyn Error>> {
    let upstream = "[fe80::53%7]:53".parse::<SocketAddr>()?;
    let query = query_bytes(0x5566, "dns.example.")?;
    let mut value = request(query, upstream, DnsRouteKind::Direct);
    value.upstreams[0].ip_address = "fe80::53".to_owned();
    value.upstreams[0].scope_id = 7;
    let parsed = ValidatedDnsRequest::parse(value)?;
    assert_eq!(parsed.upstreams(), &[upstream]);
    Ok(())
}
