use std::{fmt, net::IpAddr};

use crate::OutboundError;

#[derive(Clone, Eq, PartialEq)]
pub struct ProxyEndpoint {
    host: String,
    port: u16,
}

impl ProxyEndpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Result<Self, OutboundError> {
        let host = host.into();
        if host.is_empty()
            || host.trim() != host
            || port == 0
            || host
                .bytes()
                .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'@' | b'#' | b'?'))
        {
            return Err(OutboundError::InvalidEndpoint);
        }
        Ok(Self { host, port })
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn authority(&self) -> String {
        if self
            .host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_ipv6())
        {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

impl fmt::Debug for ProxyEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxyEndpoint")
            .field("host", &self.host)
            .field("port", &self.port)
            .finish()
    }
}
