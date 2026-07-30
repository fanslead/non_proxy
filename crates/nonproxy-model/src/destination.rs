use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use crate::ModelError;

const MAX_DOMAIN_LENGTH: usize = 253;
const MAX_DOMAIN_LABEL_LENGTH: usize = 63;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Transport {
    Tcp,
    Udp,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IpFamily {
    Ipv4,
    Ipv6,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DomainName {
    ascii: String,
    registrable: Option<String>,
}

impl DomainName {
    pub fn normalize(input: &str) -> Result<Self, ModelError> {
        let trimmed = input.trim();
        if trimmed != input {
            return Err(ModelError::DomainHasOuterWhitespace);
        }
        let without_root_dot = trimmed.strip_suffix('.').unwrap_or(trimmed);
        if without_root_dot.is_empty() {
            return Err(ModelError::EmptyDomain);
        }
        if without_root_dot.parse::<IpAddr>().is_ok() {
            return Err(ModelError::DomainIsIpAddress);
        }

        let ascii = idna::domain_to_ascii_strict(without_root_dot)
            .map_err(ModelError::InvalidIdnaDomain)?
            .to_ascii_lowercase();
        validate_ascii_domain(&ascii)?;
        let registrable = psl::domain_str(&ascii).map(str::to_owned);

        Ok(Self { ascii, registrable })
    }

    #[must_use]
    pub fn as_ascii(&self) -> &str {
        &self.ascii
    }

    #[must_use]
    pub fn registrable(&self) -> Option<&str> {
        self.registrable.as_deref()
    }
}

impl fmt::Display for DomainName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.ascii)
    }
}

impl FromStr for DomainName {
    type Err = ModelError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::normalize(input)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DomainMatchKind {
    Exact,
    Suffix,
    RegistrableDomain,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DomainMatcher {
    kind: DomainMatchKind,
    pattern: DomainName,
}

impl DomainMatcher {
    pub fn new(kind: DomainMatchKind, pattern: &str) -> Result<Self, ModelError> {
        let pattern = DomainName::normalize(pattern)?;
        if kind == DomainMatchKind::RegistrableDomain
            && pattern.registrable() != Some(pattern.as_ascii())
        {
            return Err(ModelError::InvalidRegistrableDomainPattern);
        }
        Ok(Self { kind, pattern })
    }

    #[must_use]
    pub const fn kind(&self) -> DomainMatchKind {
        self.kind
    }

    #[must_use]
    pub const fn pattern(&self) -> &DomainName {
        &self.pattern
    }

    #[must_use]
    pub fn matches(&self, destination: &DomainName) -> bool {
        match self.kind {
            DomainMatchKind::Exact => destination == &self.pattern,
            DomainMatchKind::Suffix => {
                destination.ascii == self.pattern.ascii
                    || destination
                        .ascii
                        .strip_suffix(&self.pattern.ascii)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
            DomainMatchKind::RegistrableDomain => {
                destination.registrable() == Some(self.pattern.as_ascii())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Cidr {
    network: IpAddr,
    prefix_length: u8,
}

impl Cidr {
    pub fn new(network: IpAddr, prefix_length: u8) -> Result<Self, ModelError> {
        let network = match network {
            IpAddr::V4(address) => {
                if prefix_length > 32 {
                    return Err(ModelError::InvalidCidrPrefix);
                }
                IpAddr::V4(mask_ipv4(address, prefix_length))
            }
            IpAddr::V6(address) => {
                if prefix_length > 128 {
                    return Err(ModelError::InvalidCidrPrefix);
                }
                IpAddr::V6(mask_ipv6(address, prefix_length))
            }
        };

        Ok(Self {
            network,
            prefix_length,
        })
    }

    #[must_use]
    pub const fn network(&self) -> IpAddr {
        self.network
    }

    #[must_use]
    pub const fn prefix_length(&self) -> u8 {
        self.prefix_length
    }

    #[must_use]
    pub fn contains(&self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                mask_ipv4(address, self.prefix_length) == network
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                mask_ipv6(address, self.prefix_length) == network
            }
            _ => false,
        }
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.network, self.prefix_length)
    }
}

impl FromStr for Cidr {
    type Err = ModelError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let Some((network, prefix)) = input.split_once('/') else {
            return Err(ModelError::InvalidCidrShape);
        };
        let network = network
            .parse::<IpAddr>()
            .map_err(ModelError::InvalidCidrAddress)?;
        let prefix = prefix
            .parse::<u8>()
            .map_err(ModelError::InvalidCidrPrefixText)?;
        Self::new(network, prefix)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PortRange {
    first: u16,
    last: u16,
}

impl PortRange {
    pub fn new(first: u16, last: u16) -> Result<Self, ModelError> {
        if first == 0 || last < first {
            return Err(ModelError::InvalidPortRange);
        }
        Ok(Self { first, last })
    }

    #[must_use]
    pub const fn first(&self) -> u16 {
        self.first
    }

    #[must_use]
    pub const fn last(&self) -> u16 {
        self.last
    }

    #[must_use]
    pub const fn contains(&self, port: u16) -> bool {
        port >= self.first && port <= self.last
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Destination {
    hostname: Option<String>,
    domain: Option<DomainName>,
    ip: Option<IpAddr>,
    port: u16,
    transport: Transport,
    interface_name: Option<String>,
}

impl Destination {
    pub fn new(
        hostname: Option<&str>,
        ip: Option<IpAddr>,
        port: u16,
        transport: Transport,
    ) -> Result<Self, ModelError> {
        if hostname.is_none() && ip.is_none() {
            return Err(ModelError::DestinationMissingAddress);
        }
        if port == 0 {
            return Err(ModelError::InvalidDestinationPort);
        }

        let hostname = hostname.map(str::to_owned);
        let domain = match hostname.as_deref() {
            Some(value) => Some(DomainName::normalize(value)?),
            None => None,
        };

        Ok(Self {
            hostname,
            domain,
            ip,
            port,
            transport,
            interface_name: None,
        })
    }

    #[must_use]
    pub fn with_interface_name(mut self, interface_name: impl Into<String>) -> Self {
        self.interface_name = Some(interface_name.into());
        self
    }

    #[must_use]
    pub fn hostname(&self) -> Option<&str> {
        self.hostname.as_deref()
    }

    #[must_use]
    pub const fn domain(&self) -> Option<&DomainName> {
        self.domain.as_ref()
    }

    #[must_use]
    pub const fn ip(&self) -> Option<IpAddr> {
        self.ip
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.transport
    }

    #[must_use]
    pub const fn ip_family(&self) -> Option<IpFamily> {
        match self.ip {
            Some(IpAddr::V4(_)) => Some(IpFamily::Ipv4),
            Some(IpAddr::V6(_)) => Some(IpFamily::Ipv6),
            None => None,
        }
    }

    #[must_use]
    pub fn interface_name(&self) -> Option<&str> {
        self.interface_name.as_deref()
    }
}

fn validate_ascii_domain(ascii: &str) -> Result<(), ModelError> {
    if ascii.len() > MAX_DOMAIN_LENGTH {
        return Err(ModelError::DomainTooLong);
    }
    if ascii.split('.').any(|label| {
        label.is_empty()
            || label.len() > MAX_DOMAIN_LABEL_LENGTH
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
    }) {
        return Err(ModelError::InvalidDomainLabel);
    }
    Ok(())
}

fn mask_ipv4(address: Ipv4Addr, prefix_length: u8) -> Ipv4Addr {
    let mask = if prefix_length == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_length)
    };
    Ipv4Addr::from(u32::from(address) & mask)
}

fn mask_ipv6(address: Ipv6Addr, prefix_length: u8) -> Ipv6Addr {
    let mask = if prefix_length == 0 {
        0
    } else {
        u128::MAX << (128 - prefix_length)
    };
    Ipv6Addr::from(u128::from(address) & mask)
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        net::{Ipv4Addr, Ipv6Addr},
    };

    use proptest::prelude::*;

    use super::*;

    fn must_domain(input: &str) -> DomainName {
        match DomainName::normalize(input) {
            Ok(domain) => domain,
            Err(error) => panic!("测试域名规范化失败: {error}"),
        }
    }

    fn must_cidr(input: &str) -> Cidr {
        match input.parse::<Cidr>() {
            Ok(cidr) => cidr,
            Err(error) => panic!("测试 CIDR 解析失败: {error}"),
        }
    }

    #[test]
    fn domain_normalization_handles_idn_case_and_root_dot() {
        let domain = must_domain("BÜCHER.Example.");

        assert_eq!(domain.as_ascii(), "xn--bcher-kva.example");
    }

    #[test]
    fn public_suffix_uses_real_registrable_domain_rules() {
        let domain = must_domain("www.service.example.co.uk");

        assert_eq!(domain.registrable(), Some("example.co.uk"));
    }

    #[test]
    fn suffix_match_respects_label_boundaries() {
        let matcher_result = DomainMatcher::new(DomainMatchKind::Suffix, "example.com");
        let Ok(matcher) = matcher_result else {
            panic!("测试域名匹配器创建失败: {matcher_result:?}");
        };

        assert!(matcher.matches(&must_domain("api.example.com")));
        assert!(!matcher.matches(&must_domain("notexample.com")));
    }

    #[test]
    fn registrable_matcher_rejects_a_subdomain_pattern() {
        assert!(matches!(
            DomainMatcher::new(DomainMatchKind::RegistrableDomain, "www.example.com"),
            Err(ModelError::InvalidRegistrableDomainPattern)
        ));
    }

    #[test]
    fn hostname_rules_reject_service_record_syntax() {
        let result = DomainName::normalize("_service.example.com");
        let Err(error) = result else {
            panic!("服务记录语法不应被接受");
        };

        assert_eq!(error.code(), "NP_MODEL_DOMAIN_IDNA_INVALID");
        assert!(error.source().is_some());
    }

    #[test]
    fn domain_rejects_ambiguous_whitespace_and_multiple_root_dots() {
        assert!(matches!(
            DomainName::normalize(" example.com"),
            Err(ModelError::DomainHasOuterWhitespace)
        ));
        assert!(DomainName::normalize("example.com..").is_err());
    }

    #[test]
    fn cidr_is_canonical_and_family_scoped() {
        let network = must_cidr("192.168.1.42/24");

        assert_eq!(network.to_string(), "192.168.1.0/24");
        assert!(network.contains(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 200))));
        assert!(!network.contains(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn cidr_parse_error_retains_its_source() {
        let result = "not-an-ip/24".parse::<Cidr>();
        let Err(error) = result else {
            panic!("无效 CIDR 不应被接受");
        };

        assert_eq!(error.code(), "NP_MODEL_CIDR_TEXT_INVALID");
        assert!(error.source().is_some());
    }

    proptest! {
        #[test]
        fn ipv4_cidr_always_contains_its_canonical_network(
            address in any::<u32>(),
            prefix in 0_u8..=32,
        ) {
            let result = Cidr::new(IpAddr::V4(Ipv4Addr::from(address)), prefix);
            prop_assert!(result.is_ok());
            if let Ok(cidr) = result {
                prop_assert!(cidr.contains(cidr.network()));
            }
        }

        #[test]
        fn ipv6_cidr_always_contains_its_canonical_network(
            address in any::<u128>(),
            prefix in 0_u8..=128,
        ) {
            let result = Cidr::new(IpAddr::V6(Ipv6Addr::from(address)), prefix);
            prop_assert!(result.is_ok());
            if let Ok(cidr) = result {
                prop_assert!(cidr.contains(cidr.network()));
            }
        }
    }
}
