use nonproxy_model::{NetworkProfileId, OutboundId};

use crate::DnsName;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DnsRoute {
    Direct,
    Proxy(OutboundId),
    System,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DnsCacheKey {
    pub(crate) qname: DnsName,
    pub(crate) qtype: u16,
    pub(crate) route: DnsRoute,
    pub(crate) network_profile: Option<NetworkProfileId>,
    pub(crate) query_variant: [u8; 32],
}

impl DnsCacheKey {
    #[must_use]
    pub const fn route(&self) -> &DnsRoute {
        &self.route
    }

    #[must_use]
    pub const fn network_profile(&self) -> Option<&NetworkProfileId> {
        self.network_profile.as_ref()
    }

    #[must_use]
    pub const fn qname(&self) -> &DnsName {
        &self.qname
    }

    #[must_use]
    pub const fn qtype(&self) -> u16 {
        self.qtype
    }
}
