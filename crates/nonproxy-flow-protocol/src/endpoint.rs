use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use nonproxy_model::DomainName;

use crate::FlowProtocolError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowEndpoint {
    Domain(DomainName, u16),
    Ip(SocketAddr),
}

impl FlowEndpoint {
    pub fn new(host: &str, port: u16) -> Result<Self, FlowProtocolError> {
        if port == 0 {
            return Err(FlowProtocolError::InvalidPayload);
        }
        if let Ok(address) = host.parse::<IpAddr>() {
            return Ok(Self::Ip(SocketAddr::new(address, port)));
        }
        Ok(Self::Domain(DomainName::normalize(host)?, port))
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        match self {
            Self::Domain(_, port) => *port,
            Self::Ip(address) => address.port(),
        }
    }

    #[must_use]
    pub fn host(&self) -> String {
        match self {
            Self::Domain(domain, _) => domain.as_ascii().to_owned(),
            Self::Ip(address) => address.ip().to_string(),
        }
    }

    pub(crate) fn encode_into(&self, output: &mut Vec<u8>) -> Result<(), FlowProtocolError> {
        match self {
            Self::Ip(SocketAddr::V4(address)) => {
                output.push(1);
                output.extend_from_slice(&address.ip().octets());
                output.extend_from_slice(&address.port().to_be_bytes());
            }
            Self::Domain(domain, port) => {
                let bytes = domain.as_ascii().as_bytes();
                let length =
                    u8::try_from(bytes.len()).map_err(|_| FlowProtocolError::InvalidPayload)?;
                output.push(3);
                output.push(length);
                output.extend_from_slice(bytes);
                output.extend_from_slice(&port.to_be_bytes());
            }
            Self::Ip(SocketAddr::V6(address)) => {
                output.push(4);
                output.extend_from_slice(&address.ip().octets());
                output.extend_from_slice(&address.port().to_be_bytes());
            }
        }
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<(Self, usize), FlowProtocolError> {
        let Some(kind) = input.first().copied() else {
            return Err(FlowProtocolError::InvalidPayload);
        };
        match kind {
            1 => decode_ipv4(input),
            3 => decode_domain(input),
            4 => decode_ipv6(input),
            _ => Err(FlowProtocolError::InvalidPayload),
        }
    }
}

fn decode_ipv4(input: &[u8]) -> Result<(FlowEndpoint, usize), FlowProtocolError> {
    if input.len() < 7 {
        return Err(FlowProtocolError::InvalidPayload);
    }
    let address = Ipv4Addr::new(input[1], input[2], input[3], input[4]);
    let port = u16::from_be_bytes([input[5], input[6]]);
    endpoint_from_ip(IpAddr::V4(address), port, 7)
}

fn decode_ipv6(input: &[u8]) -> Result<(FlowEndpoint, usize), FlowProtocolError> {
    if input.len() < 19 {
        return Err(FlowProtocolError::InvalidPayload);
    }
    let octets: [u8; 16] = input[1..17]
        .try_into()
        .map_err(|_| FlowProtocolError::InvalidPayload)?;
    let port = u16::from_be_bytes([input[17], input[18]]);
    endpoint_from_ip(IpAddr::V6(Ipv6Addr::from(octets)), port, 19)
}

fn decode_domain(input: &[u8]) -> Result<(FlowEndpoint, usize), FlowProtocolError> {
    let Some(length) = input.get(1).copied().map(usize::from) else {
        return Err(FlowProtocolError::InvalidPayload);
    };
    let total = 2_usize
        .checked_add(length)
        .and_then(|value| value.checked_add(2))
        .ok_or(FlowProtocolError::InvalidPayload)?;
    if length == 0 || input.len() < total {
        return Err(FlowProtocolError::InvalidPayload);
    }
    let domain = std::str::from_utf8(&input[2..2 + length])
        .map_err(|_| FlowProtocolError::InvalidPayload)?;
    let port = u16::from_be_bytes([input[total - 2], input[total - 1]]);
    if port == 0 {
        return Err(FlowProtocolError::InvalidPayload);
    }
    Ok((
        FlowEndpoint::Domain(DomainName::normalize(domain)?, port),
        total,
    ))
}

fn endpoint_from_ip(
    address: IpAddr,
    port: u16,
    consumed: usize,
) -> Result<(FlowEndpoint, usize), FlowProtocolError> {
    if port == 0 {
        return Err(FlowProtocolError::InvalidPayload);
    }
    Ok((FlowEndpoint::Ip(SocketAddr::new(address, port)), consumed))
}
