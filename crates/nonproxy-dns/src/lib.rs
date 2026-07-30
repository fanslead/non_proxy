mod cache;
mod error;
mod message;
mod name;
mod query_builder;
mod route;
mod synthetic;
mod synthetic_response;

pub use cache::{CachedDnsResponse, PartitionedDnsCache};
pub use error::DnsError;
pub use message::{
    DnsAddressObservation, DnsCnameObservation, DnsQuestion, ParsedDnsQuery, ParsedDnsResponse,
};
pub use name::DnsName;
pub use query_builder::address_query;
pub use route::{DnsCacheKey, DnsRoute};
pub use synthetic::{SYNTHETIC_IPV4_CAPACITY, SyntheticAddressFamily, SyntheticAddressSpace};
pub use synthetic_response::{
    SYNTHETIC_DNS_TTL_SECONDS, refused_response, server_failure_response,
    synthetic_address_response, synthetic_nodata_response,
};
