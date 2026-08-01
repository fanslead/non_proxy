use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use nonproxy_windows_wfp::RedirectContext;

use crate::GatewayError;

const AF_INET: u16 = 2;
const AF_INET6: u16 = 23;

pub fn original_remote(context: &RedirectContext) -> Result<SocketAddr, GatewayError> {
    decode_socket_address(context.original_remote())
        .ok_or(GatewayError::InvalidContract("WFP 原始目标地址无效"))
}

fn decode_socket_address(bytes: &[u8; 128]) -> Option<SocketAddr> {
    let family = u16::from_le_bytes([bytes[0], bytes[1]]);
    let port = u16::from_be_bytes([bytes[2], bytes[3]]);
    if port == 0 {
        return None;
    }
    match family {
        AF_INET => Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7])),
            port,
        )),
        AF_INET6 => {
            let octets: [u8; 16] = bytes[8..24].try_into().ok()?;
            let scope_id = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(octets)), port)).map(|address| {
                match address {
                    SocketAddr::V6(mut value) => {
                        value.set_scope_id(scope_id);
                        SocketAddr::V6(value)
                    }
                    value => value,
                }
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_socket_address;

    #[test]
    fn decodes_ipv4_sockaddr_storage() {
        let mut bytes = [0_u8; 128];
        bytes[0..2].copy_from_slice(&2_u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&443_u16.to_be_bytes());
        bytes[4..8].copy_from_slice(&[203, 0, 113, 7]);

        assert_eq!(
            decode_socket_address(&bytes).map(|value| value.to_string()),
            Some("203.0.113.7:443".to_owned())
        );
    }
}
