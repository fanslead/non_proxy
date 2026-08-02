use std::{error::Error, net::SocketAddr, sync::Arc};

use hickory_proto::{
    op::{Message, MessageType, OpCode, Query},
    rr::{Name, RData, Record, RecordType, rdata::A},
};
use nonproxy_model::{OutboundGroupId, OutboundId};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1::{AppIdentity, Platform},
    events::v1::RuntimeState,
    provider::v1::{DnsRouteKind, DnsUpstreamEndpoint, ResolveDnsRequest},
};
use nonproxy_storage::{
    OutboundGroup, OutboundGroupStrategy, OutboundKind, OutboundReference, PolicyDatabase,
    ProviderAck,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use super::DnsResolutionService;
use crate::{
    Gateway,
    credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
};

#[tokio::test]
async fn group_dns_uses_and_reports_the_selected_concrete_outbound() -> Result<(), Box<dyn Error>> {
    let proxy = TcpListener::bind(("127.0.0.1", 0)).await?;
    let proxy_port = proxy.local_addr()?.port();
    let fixture = tokio::spawn(proxy_dns_fixture(proxy));
    let gateway = active_group_gateway(proxy_port).await?;
    let now = crate::clock::unix_time_ms()?;
    for observed_at in [now, now] {
        gateway.report_outbound_health(
            outbound_id("backup")?,
            1,
            RuntimeState::Ready,
            Some(7),
            observed_at,
        )?;
    }
    let credentials: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    let service = DnsResolutionService::new(gateway, credentials);
    let query = dns_query(0x7788)?;
    let upstream = "203.0.113.53:53".parse::<SocketAddr>()?;
    let mut request = request(query, upstream);
    request.requested_outbound_group_id = "automatic".to_owned();

    let result = service.resolve(request).await?;
    fixture.await??;

    assert_eq!(result.outbound_id().map(OutboundId::as_str), Some("backup"));
    assert!(!result.cache_hit);
    assert_eq!(Message::from_vec(&result.dns_message)?.id, 0x7788);
    Ok(())
}

#[test]
fn group_dns_target_is_mutually_exclusive_with_a_concrete_outbound() -> Result<(), Box<dyn Error>> {
    let mut request = request(dns_query(0x8899)?, "203.0.113.53:53".parse()?);
    request.requested_outbound_group_id = "automatic".to_owned();
    request.requested_outbound_id = "primary".to_owned();

    assert!(super::request::ValidatedDnsRequest::parse(request).is_err());
    Ok(())
}

async fn active_group_gateway(proxy_port: u16) -> Result<Gateway, Box<dyn Error>> {
    let database = PolicyDatabase::open_in_memory(1)?;
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let outbounds = [("primary", 9_u16), ("backup", proxy_port)]
        .into_iter()
        .map(|(id, port)| {
            Ok((
                OutboundReference::new(
                    outbound_id(id)?,
                    OutboundKind::HttpConnect,
                    Some("127.0.0.1"),
                    Some(port),
                    None,
                    1,
                )?,
                None,
            ))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    gateway.save_outbounds(outbounds).await?;
    gateway
        .save_outbound_group(
            OutboundGroup::new(
                OutboundGroupId::new("automatic")?,
                "自动切换",
                OutboundGroupStrategy::Failover,
                vec![outbound_id("primary")?, outbound_id("backup")?],
                1,
            )?,
            None,
        )
        .await?;
    let published = gateway.compile_and_stage().await?;
    let required = vec!["transparent-proxy".to_owned(), "dns-proxy".to_owned()];
    for provider in &required {
        gateway
            .acknowledge_provider_snapshot(
                1,
                ProviderAck::loaded(
                    provider,
                    1,
                    *published.artifact().content_hash(),
                    crate::clock::unix_time_ms()?,
                )?,
                required.clone(),
            )
            .await?;
    }
    Ok(gateway)
}

fn request(query: Vec<u8>, upstream: SocketAddr) -> ResolveDnsRequest {
    ResolveDnsRequest {
        query_id: "group-query".to_owned(),
        app: Some(AppIdentity {
            platform: Platform::Macos as i32,
            stable_id: "com.example.browser".to_owned(),
            ..Default::default()
        }),
        qname: "dns.example".to_owned(),
        qtype: u32::from(u16::from(RecordType::A)),
        network_profile_id: "office".to_owned(),
        dns_message: query,
        requested_route: DnsRouteKind::Proxy as i32,
        requested_outbound_id: String::new(),
        requested_outbound_group_id: String::new(),
        upstreams: vec![DnsUpstreamEndpoint {
            ip_address: upstream.ip().to_string(),
            port: u32::from(upstream.port()),
            scope_id: 0,
        }],
        snapshot_version: 1,
        ..Default::default()
    }
}

fn dns_query(id: u16) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut message = Message::new(id, MessageType::Query, OpCode::Query);
    message.add_query(Query::query(
        Name::from_ascii("dns.example.")?,
        RecordType::A,
    ));
    Ok(message.to_vec()?)
}

async fn proxy_dns_fixture(listener: TcpListener) -> Result<(), std::io::Error> {
    let (mut stream, _) = listener.accept().await?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        header.push(stream.read_u8().await?);
        if header.len() > 16 * 1024 {
            return Err(std::io::Error::other("CONNECT header 过长"));
        }
    }
    if !String::from_utf8_lossy(&header).contains("CONNECT 203.0.113.53:53") {
        return Err(std::io::Error::other("CONNECT 目标错误"));
    }
    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;
    let length = stream.read_u16().await?;
    let mut query = vec![0_u8; usize::from(length)];
    stream.read_exact(&mut query).await?;
    let parsed = Message::from_vec(&query).map_err(std::io::Error::other)?;
    let question = parsed
        .queries
        .first()
        .ok_or_else(|| std::io::Error::other("DNS question 缺失"))?;
    let mut response = Message::response(parsed.id, OpCode::Query);
    response.add_query(question.clone());
    response.add_answer(Record::from_rdata(
        question.name().clone(),
        30,
        RData::A(A::new(198, 51, 100, 10)),
    ));
    let bytes = response.to_vec().map_err(std::io::Error::other)?;
    stream
        .write_all(
            &u16::try_from(bytes.len())
                .map_err(std::io::Error::other)?
                .to_be_bytes(),
        )
        .await?;
    stream.write_all(&bytes).await
}

fn outbound_id(value: &str) -> Result<OutboundId, nonproxy_model::ModelError> {
    OutboundId::new(value)
}
