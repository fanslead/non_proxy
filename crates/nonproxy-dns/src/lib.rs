mod cache;
mod error;
mod message;
mod route;

pub use cache::{CachedDnsResponse, PartitionedDnsCache};
pub use error::DnsError;
pub use message::{
    DnsAddressObservation, DnsCnameObservation, DnsQuestion, ParsedDnsQuery, ParsedDnsResponse,
};
pub use route::{DnsCacheKey, DnsRoute};
