mod cache;
mod error;
mod message;
mod name;
mod route;

pub use cache::{CachedDnsResponse, PartitionedDnsCache};
pub use error::DnsError;
pub use message::{
    DnsAddressObservation, DnsCnameObservation, DnsQuestion, ParsedDnsQuery, ParsedDnsResponse,
};
pub use name::DnsName;
pub use route::{DnsCacheKey, DnsRoute};
