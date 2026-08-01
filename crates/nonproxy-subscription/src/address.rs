use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[must_use]
pub fn is_public_destination(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(value) => is_public_ipv4(value),
        IpAddr::V6(value) => is_public_ipv6(value),
    }
}

fn is_public_ipv4(value: Ipv4Addr) -> bool {
    let octets = value.octets();
    !matches!(
        octets,
        [0, ..]
            | [10, ..]
            | [100, 64..=127, ..]
            | [127, ..]
            | [169, 254, ..]
            | [172, 16..=31, ..]
            | [192, 0, 0, ..]
            | [192, 0, 2, ..]
            | [192, 88, 99, ..]
            | [192, 168, ..]
            | [198, 18..=19, ..]
            | [198, 51, 100, ..]
            | [203, 0, 113, ..]
            | [224..=255, ..]
    )
}

fn is_public_ipv6(value: Ipv6Addr) -> bool {
    if let Some(mapped) = value.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = value.segments();
    if segments[..6] == [0x0064, 0xff9b, 0, 0, 0, 0] {
        return is_public_ipv4(Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            segments[6] as u8,
            (segments[7] >> 8) as u8,
            segments[7] as u8,
        ));
    }
    (segments[0] & 0xe000) == 0x2000
        && segments[0] != 0x3ffe
        && segments[0] != 0x2002
        && !(segments[0] == 0x2001
            && (segments[1] == 0
                || segments[1] == 0x0db8
                || segments[1] == 0x0002
                || (segments[1] & 0xfff0) == 0x0010
                || (segments[1] & 0xfff0) == 0x0020))
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::is_public_destination;

    #[test]
    fn accepts_public_addresses_and_public_dns64_translation() {
        for value in [
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
            "64:ff9b::5db8:d822",
            "::ffff:93.184.216.34",
        ] {
            let address = value
                .parse::<IpAddr>()
                .unwrap_or_else(|error| panic!("测试地址解析失败: {error}"));
            assert!(is_public_destination(address), "{value}");
        }
    }

    #[test]
    fn rejects_private_local_reserved_and_documentation_ranges() {
        for value in [
            "0.0.0.0",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2001:20::1",
            "2002:7f00:1::1",
            "3ffe::1",
            "64:ff9b::a00:1",
            "::ffff:127.0.0.1",
        ] {
            let address = value
                .parse::<IpAddr>()
                .unwrap_or_else(|error| panic!("测试地址解析失败: {error}"));
            assert!(!is_public_destination(address), "{value}");
        }
    }
}
