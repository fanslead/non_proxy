use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use hickory_proto::{
    op::{Message, MessageType, OpCode, Query, ResponseCode},
    rr::{
        Name, RData, Record, RecordType,
        rdata::{A, AAAA, CNAME, SOA},
    },
};
use nonproxy_dns::{DnsRoute, ParsedDnsQuery, ParsedDnsResponse, PartitionedDnsCache};
use nonproxy_model::{NetworkProfileId, OutboundId};

fn query_message(id: u16, qname: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut message = Message::new(id, MessageType::Query, OpCode::Query);
    message.add_query(Query::query(Name::from_ascii(qname)?, RecordType::A));
    Ok(message.to_vec()?)
}

fn positive_response(
    query: &ParsedDnsQuery,
    qname: &str,
    id: u16,
    ttl: u32,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let owner = Name::from_ascii(qname)?;
    let canonical = Name::from_ascii(format!("edge.{qname}"))?;
    let mut message = Message::response(id, OpCode::Query);
    message.add_query(Query::query(owner.clone(), RecordType::A));
    message.add_answer(Record::from_rdata(
        owner.clone(),
        ttl,
        RData::CNAME(CNAME(canonical.clone())),
    ));
    message.add_answer(Record::from_rdata(
        canonical.clone(),
        ttl.saturating_add(30),
        RData::A(A::new(203, 0, 113, 8)),
    ));
    message.add_answer(Record::from_rdata(
        canonical,
        ttl.saturating_add(60),
        RData::AAAA(AAAA(Ipv6Addr::LOCALHOST)),
    ));
    let bytes = message.to_vec()?;
    assert_eq!(query.transaction_id(), id);
    Ok(bytes)
}

fn parsed_positive_response(
    query: &ParsedDnsQuery,
    qname: &str,
    ttl: u32,
) -> Result<ParsedDnsResponse, Box<dyn Error>> {
    let bytes = positive_response(query, qname, query.transaction_id(), ttl)?;
    Ok(ParsedDnsResponse::parse(query, &bytes)?)
}

#[test]
fn query_and_response_preserve_normalized_observations() -> Result<(), Box<dyn Error>> {
    let query_bytes = query_message(0x1234, "Example.COM.")?;
    let query = ParsedDnsQuery::parse(&query_bytes)?;
    assert_eq!(query.transaction_id(), 0x1234);
    assert_eq!(query.question().qname().as_ascii(), "example.com");
    assert_eq!(query.question().qtype(), u16::from(RecordType::A));

    let response = parsed_positive_response(&query, "example.com.", 60)?;
    assert_eq!(response.valid_for_seconds(), 60);
    assert_eq!(response.cnames().len(), 1);
    assert_eq!(response.cnames()[0].alias().as_ascii(), "example.com");
    assert_eq!(
        response.cnames()[0].canonical().as_ascii(),
        "edge.example.com"
    );
    assert_eq!(response.addresses().len(), 2);
    assert_eq!(
        response.addresses()[0].address(),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8))
    );
    assert_eq!(
        response.addresses()[1].address(),
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    );
    Ok(())
}

#[test]
fn cache_is_partitioned_by_route_outbound_and_network() -> Result<(), Box<dyn Error>> {
    let query = ParsedDnsQuery::parse(&query_message(7, "partition.example.")?)?;
    let response = parsed_positive_response(&query, "partition.example.", 60)?;
    let direct_key = query.cache_key(DnsRoute::Direct, Some(NetworkProfileId::new("office")?));
    let proxy_key = query.cache_key(
        DnsRoute::Proxy(OutboundId::new("proxy-a")?),
        Some(NetworkProfileId::new("office")?),
    );
    let other_network_key =
        query.cache_key(DnsRoute::Direct, Some(NetworkProfileId::new("mobile")?));
    let cache = PartitionedDnsCache::new(4)?;
    assert!(cache.insert(direct_key.clone(), response, 1_000)?);

    assert!(cache.get(&direct_key, 8, 1_000)?.is_some());
    assert!(cache.get(&proxy_key, 8, 1_000)?.is_none());
    assert!(cache.get(&other_network_key, 8, 1_000)?.is_none());
    Ok(())
}

#[test]
fn cache_rewrites_id_and_decrements_wire_ttl() -> Result<(), Box<dyn Error>> {
    let query = ParsedDnsQuery::parse(&query_message(9, "ttl.example.")?)?;
    let key = query.cache_key(DnsRoute::Direct, None);
    let response = parsed_positive_response(&query, "ttl.example.", 60)?;
    let cache = PartitionedDnsCache::new(2)?;
    assert!(cache.insert(key.clone(), response, 10_000)?);

    let cached = cache.get(&key, 0xBEEF, 11_500)?.ok_or("缓存条目应当存在")?;
    assert_eq!(cached.remaining_ttl_seconds(), 58);
    let message = Message::from_vec(cached.bytes())?;
    assert_eq!(message.id, 0xBEEF);
    assert_eq!(message.answers[0].ttl, 58);
    assert_eq!(message.answers[1].ttl, 88);
    assert_eq!(message.answers[2].ttl, 118);
    Ok(())
}

#[test]
fn negative_cache_uses_minimum_of_soa_ttl_and_minimum() -> Result<(), Box<dyn Error>> {
    let query = ParsedDnsQuery::parse(&query_message(11, "missing.example.")?)?;
    let owner = Name::from_ascii("example.")?;
    let mut message = Message::response(query.transaction_id(), OpCode::Query);
    message.metadata.response_code = ResponseCode::NXDomain;
    message.add_query(Query::query(
        Name::from_ascii("missing.example.")?,
        RecordType::A,
    ));
    message.add_authority(Record::from_rdata(
        owner,
        600,
        RData::SOA(SOA::new(
            Name::from_ascii("ns.example.")?,
            Name::from_ascii("hostmaster.example.")?,
            1,
            3600,
            600,
            86_400,
            120,
        )),
    ));

    let response = ParsedDnsResponse::parse(&query, &message.to_vec()?)?;
    assert_eq!(response.valid_for_seconds(), 120);
    Ok(())
}

#[test]
fn zero_ttl_response_is_not_cached() -> Result<(), Box<dyn Error>> {
    let query = ParsedDnsQuery::parse(&query_message(13, "zero.example.")?)?;
    let key = query.cache_key(DnsRoute::System, None);
    let response = parsed_positive_response(&query, "zero.example.", 0)?;
    let cache = PartitionedDnsCache::new(2)?;

    assert!(!cache.insert(key, response, 0)?);
    assert_eq!(cache.len()?, 0);
    Ok(())
}

#[test]
fn capacity_evicts_existing_entry() -> Result<(), Box<dyn Error>> {
    let first_query = ParsedDnsQuery::parse(&query_message(15, "first.example.")?)?;
    let second_query = ParsedDnsQuery::parse(&query_message(16, "second.example.")?)?;
    let first_key = first_query.cache_key(DnsRoute::Direct, None);
    let second_key = second_query.cache_key(DnsRoute::Direct, None);
    let cache = PartitionedDnsCache::new(1)?;
    assert!(cache.insert(
        first_key.clone(),
        parsed_positive_response(&first_query, "first.example.", 60)?,
        0,
    )?);
    assert!(cache.insert(
        second_key.clone(),
        parsed_positive_response(&second_query, "second.example.", 60)?,
        1,
    )?);

    assert!(cache.get(&first_key, 17, 1)?.is_none());
    assert!(cache.get(&second_key, 17, 1)?.is_some());
    Ok(())
}

#[test]
fn invalid_query_shapes_are_rejected() -> Result<(), Box<dyn Error>> {
    let mut query = Message::new(21, MessageType::Query, OpCode::Query);
    query.add_query(Query::query(
        Name::from_ascii("one.example.")?,
        RecordType::A,
    ));
    query.add_query(Query::query(
        Name::from_ascii("two.example.")?,
        RecordType::AAAA,
    ));
    assert!(ParsedDnsQuery::parse(&query.to_vec()?).is_err());
    assert!(ParsedDnsQuery::parse(&[0_u8; 11]).is_err());
    Ok(())
}

#[test]
fn dns_names_accept_service_labels_and_root() -> Result<(), Box<dyn Error>> {
    let service = ParsedDnsQuery::parse(&query_message(22, "_dns-sd._udp.local.")?)?;
    assert_eq!(service.question().qname().as_ascii(), "_dns-sd._udp.local");
    let root = ParsedDnsQuery::parse(&query_message(23, ".")?)?;
    assert!(root.question().qname().is_root());
    Ok(())
}
