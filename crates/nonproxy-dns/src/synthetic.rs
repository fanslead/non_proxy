use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use nonproxy_model::DomainName;
use sha2::{Digest, Sha256};

use crate::DnsError;

const SYNTHETIC_IPV4_FIRST: u32 = u32::from_be_bytes([198, 18, 0, 1]);
const SYNTHETIC_IPV4_LAST: u32 = u32::from_be_bytes([198, 19, 255, 254]);
pub const SYNTHETIC_IPV4_CAPACITY: u32 = SYNTHETIC_IPV4_LAST - SYNTHETIC_IPV4_FIRST + 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SyntheticAddressFamily {
    Ipv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyntheticAddressSpace {
    ipv6_prefix: u128,
}

impl SyntheticAddressSpace {
    pub fn new(ipv6_prefix: Ipv6Addr) -> Result<Self, DnsError> {
        let value = u128::from(ipv6_prefix);
        let prefix = value & (u128::MAX << 64);
        let first_octet = ipv6_prefix.octets()[0];
        if first_octet != 0xfd || value != prefix {
            return Err(DnsError::SyntheticAddressSpace);
        }
        Ok(Self {
            ipv6_prefix: prefix,
        })
    }

    #[must_use]
    pub const fn capacity(self, _family: SyntheticAddressFamily) -> u32 {
        SYNTHETIC_IPV4_CAPACITY
    }

    pub fn candidate(
        self,
        domain: &DomainName,
        family: SyntheticAddressFamily,
        attempt: u32,
    ) -> Result<IpAddr, DnsError> {
        if attempt >= self.capacity(family) {
            return Err(DnsError::SyntheticAddressExhausted);
        }
        let initial = initial_slot(domain, family);
        let slot = initial.wrapping_add(attempt) % self.capacity(family);
        Ok(match family {
            SyntheticAddressFamily::Ipv4 => IpAddr::V4(Ipv4Addr::from(SYNTHETIC_IPV4_FIRST + slot)),
            SyntheticAddressFamily::Ipv6 => {
                IpAddr::V6(Ipv6Addr::from(self.ipv6_prefix + u128::from(slot) + 1))
            }
        })
    }

    #[must_use]
    pub fn contains(self, address: IpAddr) -> bool {
        match address {
            IpAddr::V4(value) => {
                let value = u32::from(value);
                (SYNTHETIC_IPV4_FIRST..=SYNTHETIC_IPV4_LAST).contains(&value)
            }
            IpAddr::V6(value) => {
                let value = u128::from(value);
                let first = self.ipv6_prefix + 1;
                let last = self.ipv6_prefix + u128::from(SYNTHETIC_IPV4_CAPACITY);
                (first..=last).contains(&value)
            }
        }
    }
}

fn initial_slot(domain: &DomainName, family: SyntheticAddressFamily) -> u32 {
    let mut hasher = Sha256::new();
    hasher.update(b"nonproxy.synthetic-address.v1\0");
    hasher.update(match family {
        SyntheticAddressFamily::Ipv4 => b"4",
        SyntheticAddressFamily::Ipv6 => b"6",
    });
    hasher.update(b"\0");
    hasher.update(domain.as_ascii().as_bytes());
    let digest = hasher.finalize();
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    value % SYNTHETIC_IPV4_CAPACITY
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain(value: &str) -> DomainName {
        DomainName::normalize(value).unwrap_or_else(|error| panic!("测试域名无效: {error}"))
    }

    fn space() -> SyntheticAddressSpace {
        SyntheticAddressSpace::new(
            "fd42:4e50:5258:5901::"
                .parse()
                .unwrap_or(Ipv6Addr::LOCALHOST),
        )
        .unwrap_or_else(|error| panic!("测试地址空间无效: {error}"))
    }

    #[test]
    fn candidate_is_stable_bounded_and_family_separated() {
        let first = space()
            .candidate(&domain("api.example"), SyntheticAddressFamily::Ipv4, 0)
            .unwrap_or_else(|error| panic!("IPv4 候选生成失败: {error}"));
        let repeated = space()
            .candidate(&domain("api.example"), SyntheticAddressFamily::Ipv4, 0)
            .unwrap_or_else(|error| panic!("IPv4 候选重复生成失败: {error}"));
        let next = space()
            .candidate(&domain("api.example"), SyntheticAddressFamily::Ipv4, 1)
            .unwrap_or_else(|error| panic!("IPv4 后继候选生成失败: {error}"));
        let ipv6 = space()
            .candidate(&domain("api.example"), SyntheticAddressFamily::Ipv6, 0)
            .unwrap_or_else(|error| panic!("IPv6 候选生成失败: {error}"));

        assert_eq!(first, repeated);
        assert_ne!(first, next);
        assert!(first.is_ipv4());
        assert!(ipv6.is_ipv6());
        assert!(space().contains(first));
        assert!(space().contains(ipv6));
    }

    #[test]
    fn rejects_non_ula_or_non_prefix_ipv6_values() {
        assert!(SyntheticAddressSpace::new(Ipv6Addr::LOCALHOST).is_err());
        assert!(
            SyntheticAddressSpace::new("fd42::1".parse().unwrap_or(Ipv6Addr::LOCALHOST)).is_err()
        );
    }

    #[test]
    fn attempt_cannot_wrap_outside_the_pool() {
        let result = space().candidate(
            &domain("api.example"),
            SyntheticAddressFamily::Ipv4,
            SYNTHETIC_IPV4_CAPACITY,
        );

        assert!(matches!(result, Err(DnsError::SyntheticAddressExhausted)));
    }
}
