use nonproxy_model::{IpFamily, Transport};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutboundCapabilities {
    tcp: bool,
    udp: bool,
    ipv4: bool,
    ipv6: bool,
}

impl OutboundCapabilities {
    #[must_use]
    pub const fn new(tcp: bool, udp: bool, ipv4: bool, ipv6: bool) -> Self {
        Self {
            tcp,
            udp,
            ipv4,
            ipv6,
        }
    }

    #[must_use]
    pub const fn full() -> Self {
        Self::new(true, true, true, true)
    }

    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self::new(
            self.tcp && other.tcp,
            self.udp && other.udp,
            self.ipv4 && other.ipv4,
            self.ipv6 && other.ipv6,
        )
    }

    #[must_use]
    pub const fn supports_transport(self, transport: Transport) -> bool {
        match transport {
            Transport::Tcp => self.tcp,
            Transport::Udp => self.udp,
        }
    }

    #[must_use]
    pub const fn supports_family(self, family: IpFamily) -> bool {
        match family {
            IpFamily::Ipv4 => self.ipv4,
            IpFamily::Ipv6 => self.ipv6,
        }
    }
}
