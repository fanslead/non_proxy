use hickory_proto::{
    op::{Message, MessageType, OpCode, Query},
    rr::{Name, RecordType},
};
use nonproxy_model::DomainName;

use crate::{DnsError, SyntheticAddressFamily};

pub fn address_query(
    transaction_id: u16,
    domain: &DomainName,
    family: SyntheticAddressFamily,
) -> Result<Vec<u8>, DnsError> {
    let name = Name::from_ascii(domain.as_ascii()).map_err(|_| DnsError::Domain)?;
    let record_type = match family {
        SyntheticAddressFamily::Ipv4 => RecordType::A,
        SyntheticAddressFamily::Ipv6 => RecordType::AAAA,
    };
    let mut message = Message::new(transaction_id, MessageType::Query, OpCode::Query);
    message.metadata.recursion_desired = true;
    message.add_query(Query::query(name, record_type));
    message.to_vec().map_err(|_| DnsError::Codec)
}
