use std::net::IpAddr;

use hickory_proto::{
    op::{Message, MessageType, OpCode, ResponseCode},
    rr::{DNSClass, RData},
};
use nonproxy_model::DomainName;
use sha2::{Digest, Sha256};

use crate::{DnsCacheKey, DnsError, DnsRoute};

const MAXIMUM_MESSAGE_BYTES: usize = u16::MAX as usize;
const MAXIMUM_CACHE_TTL_SECONDS: u32 = 24 * 60 * 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsQuestion {
    qname: DomainName,
    qtype: u16,
}

impl DnsQuestion {
    #[must_use]
    pub const fn qname(&self) -> &DomainName {
        &self.qname
    }

    #[must_use]
    pub const fn qtype(&self) -> u16 {
        self.qtype
    }
}

#[derive(Clone, Debug)]
pub struct ParsedDnsQuery {
    transaction_id: u16,
    question: DnsQuestion,
    query_variant: [u8; 32],
}

impl ParsedDnsQuery {
    pub fn parse(bytes: &[u8]) -> Result<Self, DnsError> {
        validate_size(bytes)?;
        let message = Message::from_vec(bytes).map_err(|_| DnsError::Codec)?;
        if message.message_type != MessageType::Query
            || message.op_code != OpCode::Query
            || message.response_code != ResponseCode::NoError
            || message.queries.len() != 1
            || !message.answers.is_empty()
            || !message.authorities.is_empty()
            || !message.additionals.is_empty()
            || message.signature.is_some()
        {
            return Err(DnsError::InvalidQuery);
        }
        let query = &message.queries[0];
        if query.query_class() != DNSClass::IN {
            return Err(DnsError::InvalidQuery);
        }
        let qname = normalize_name(&query.name().to_ascii())?;
        let mut normalized = bytes.to_vec();
        normalized[0] = 0;
        normalized[1] = 0;
        let query_variant: [u8; 32] = Sha256::digest(normalized).into();
        Ok(Self {
            transaction_id: message.id,
            question: DnsQuestion {
                qname,
                qtype: u16::from(query.query_type()),
            },
            query_variant,
        })
    }

    #[must_use]
    pub const fn transaction_id(&self) -> u16 {
        self.transaction_id
    }

    #[must_use]
    pub const fn question(&self) -> &DnsQuestion {
        &self.question
    }

    #[must_use]
    pub fn cache_key(
        &self,
        route: DnsRoute,
        network_profile: Option<nonproxy_model::NetworkProfileId>,
    ) -> DnsCacheKey {
        DnsCacheKey {
            qname: self.question.qname.clone(),
            qtype: self.question.qtype,
            route,
            network_profile,
            query_variant: self.query_variant,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsAddressObservation {
    owner: DomainName,
    address: IpAddr,
    ttl_seconds: u32,
}

impl DnsAddressObservation {
    #[must_use]
    pub const fn owner(&self) -> &DomainName {
        &self.owner
    }

    #[must_use]
    pub const fn address(&self) -> IpAddr {
        self.address
    }

    #[must_use]
    pub const fn ttl_seconds(&self) -> u32 {
        self.ttl_seconds
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DnsCnameObservation {
    alias: DomainName,
    canonical: DomainName,
    ttl_seconds: u32,
}

impl DnsCnameObservation {
    #[must_use]
    pub const fn alias(&self) -> &DomainName {
        &self.alias
    }

    #[must_use]
    pub const fn canonical(&self) -> &DomainName {
        &self.canonical
    }

    #[must_use]
    pub const fn ttl_seconds(&self) -> u32 {
        self.ttl_seconds
    }
}

#[derive(Clone, Debug)]
pub struct ParsedDnsResponse {
    message: Message,
    valid_for_seconds: u32,
    addresses: Vec<DnsAddressObservation>,
    cnames: Vec<DnsCnameObservation>,
}

impl ParsedDnsResponse {
    pub fn parse(query: &ParsedDnsQuery, bytes: &[u8]) -> Result<Self, DnsError> {
        validate_size(bytes)?;
        let mut message = Message::from_vec(bytes).map_err(|_| DnsError::Codec)?;
        if message.message_type != MessageType::Response
            || message.op_code != OpCode::Query
            || message.id != query.transaction_id
            || message.queries.len() != 1
            || message.signature.is_some()
        {
            return Err(DnsError::InvalidResponse);
        }
        let response_query = &message.queries[0];
        let response_name = normalize_name(&response_query.name().to_ascii())?;
        if response_query.query_class() != DNSClass::IN
            || response_name != query.question.qname
            || u16::from(response_query.query_type()) != query.question.qtype
        {
            return Err(DnsError::InvalidResponse);
        }
        let mut addresses = Vec::new();
        let mut cnames = Vec::new();
        for record in &message.answers {
            let owner = normalize_name(&record.name.to_ascii())?;
            match &record.data {
                RData::A(value) => addresses.push(DnsAddressObservation {
                    owner,
                    address: IpAddr::V4(value.0),
                    ttl_seconds: record.ttl,
                }),
                RData::AAAA(value) => addresses.push(DnsAddressObservation {
                    owner,
                    address: IpAddr::V6(value.0),
                    ttl_seconds: record.ttl,
                }),
                RData::CNAME(value) => cnames.push(DnsCnameObservation {
                    alias: owner,
                    canonical: normalize_name(&value.0.to_ascii())?,
                    ttl_seconds: record.ttl,
                }),
                _ => {}
            }
        }
        let valid_for_seconds = cache_ttl(&message);
        message.metadata.id = 0;
        Ok(Self {
            message,
            valid_for_seconds,
            addresses,
            cnames,
        })
    }

    #[must_use]
    pub const fn valid_for_seconds(&self) -> u32 {
        self.valid_for_seconds
    }

    #[must_use]
    pub fn addresses(&self) -> &[DnsAddressObservation] {
        &self.addresses
    }

    #[must_use]
    pub fn cnames(&self) -> &[DnsCnameObservation] {
        &self.cnames
    }

    pub(crate) fn bytes_for_transaction(
        &self,
        transaction_id: u16,
        elapsed_seconds: u32,
    ) -> Result<Vec<u8>, DnsError> {
        let mut message = self.message.clone();
        message.metadata.id = transaction_id;
        for record in message
            .answers
            .iter_mut()
            .chain(message.authorities.iter_mut())
            .chain(message.additionals.iter_mut())
        {
            record.ttl = record.ttl.saturating_sub(elapsed_seconds);
        }
        message.to_vec().map_err(|_| DnsError::Codec)
    }
}

fn cache_ttl(message: &Message) -> u32 {
    match message.response_code {
        ResponseCode::NoError if !message.answers.is_empty() => message
            .answers
            .iter()
            .map(|record| record.ttl)
            .min()
            .unwrap_or(0)
            .min(MAXIMUM_CACHE_TTL_SECONDS),
        ResponseCode::NoError | ResponseCode::NXDomain => message
            .authorities
            .iter()
            .filter_map(|record| match &record.data {
                RData::SOA(soa) => Some(record.ttl.min(soa.minimum)),
                _ => None,
            })
            .min()
            .unwrap_or(0)
            .min(MAXIMUM_CACHE_TTL_SECONDS),
        _ => 0,
    }
}

fn normalize_name(value: &str) -> Result<DomainName, DnsError> {
    DomainName::normalize(value).map_err(|_| DnsError::Domain)
}

fn validate_size(bytes: &[u8]) -> Result<(), DnsError> {
    if bytes.len() < 12 || bytes.len() > MAXIMUM_MESSAGE_BYTES {
        Err(DnsError::MessageSize)
    } else {
        Ok(())
    }
}
