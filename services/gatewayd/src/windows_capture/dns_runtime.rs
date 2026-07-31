use std::net::Ipv6Addr;

use crate::GatewayError;

pub enum DnsFirstCompletion {
    Shutdown,
    Server(Result<(), GatewayError>),
    Policy(Result<(), GatewayError>),
    Readiness(Result<(), GatewayError>),
}

pub fn random_ula_prefix() -> Result<Ipv6Addr, GatewayError> {
    let mut octets = [0_u8; 16];
    getrandom::fill(&mut octets[..8]).map_err(|error| GatewayError::Random(error.to_string()))?;
    octets[0] = 0xfd;
    Ok(Ipv6Addr::from(octets))
}

pub fn random_probe_domain() -> Result<String, GatewayError> {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| GatewayError::Random(error.to_string()))?;
    let encoded = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{encoded}.probe.nonproxy.invalid"))
}
