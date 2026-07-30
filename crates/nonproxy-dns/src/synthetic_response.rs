use std::net::IpAddr;

use hickory_proto::{
    op::{Message, MessageType, OpCode, ResponseCode},
    rr::{
        Name, RData, Record, RecordType,
        rdata::{A, AAAA},
    },
};

use crate::{DnsError, ParsedDnsQuery};

pub const SYNTHETIC_DNS_TTL_SECONDS: u32 = 30;

pub fn synthetic_address_response(
    query_bytes: &[u8],
    address: IpAddr,
) -> Result<Vec<u8>, DnsError> {
    let parsed = ParsedDnsQuery::parse(query_bytes)?;
    let query = Message::from_vec(query_bytes).map_err(|_| DnsError::Codec)?;
    let expected_type = match address {
        IpAddr::V4(_) => RecordType::A,
        IpAddr::V6(_) => RecordType::AAAA,
    };
    if parsed.question().qtype() != u16::from(expected_type) {
        return Err(DnsError::SyntheticAddressFamilyMismatch);
    }
    let owner =
        Name::from_ascii(parsed.question().qname().as_ascii()).map_err(|_| DnsError::Domain)?;
    let mut response = Message::response(parsed.transaction_id(), OpCode::Query);
    response.metadata.response_code = ResponseCode::NoError;
    response.metadata.recursion_desired = query.metadata.recursion_desired;
    response.metadata.recursion_available = true;
    response.metadata.checking_disabled = query.metadata.checking_disabled;
    response.add_query(query.queries[0].clone());
    response.edns = query.edns;
    response.add_answer(Record::from_rdata(
        owner,
        SYNTHETIC_DNS_TTL_SECONDS,
        match address {
            IpAddr::V4(value) => RData::A(A(value)),
            IpAddr::V6(value) => RData::AAAA(AAAA(value)),
        },
    ));
    let bytes = response.to_vec().map_err(|_| DnsError::Codec)?;
    let validated = Message::from_vec(&bytes).map_err(|_| DnsError::Codec)?;
    if validated.message_type != MessageType::Response {
        return Err(DnsError::Codec);
    }
    Ok(bytes)
}
