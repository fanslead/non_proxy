use std::fmt;

use zeroize::Zeroizing;

use crate::OutboundError;

const CREDENTIAL_VERSION: u8 = 1;

pub struct ProxyCredentials {
    username: Zeroizing<String>,
    password: Zeroizing<String>,
}

impl ProxyCredentials {
    pub fn new(username: String, password: String) -> Result<Self, OutboundError> {
        if username.is_empty()
            || password.is_empty()
            || username.len() > u8::MAX as usize
            || password.len() > u8::MAX as usize
        {
            return Err(OutboundError::InvalidCredential);
        }
        Ok(Self {
            username: Zeroizing::new(username),
            password: Zeroizing::new(password),
        })
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, OutboundError> {
        if encoded.len() < 4 || encoded[0] != CREDENTIAL_VERSION {
            return Err(OutboundError::InvalidCredential);
        }
        let username_length = usize::from(encoded[1]);
        let password_start = 2_usize
            .checked_add(username_length)
            .ok_or(OutboundError::InvalidCredential)?;
        if username_length == 0 || password_start >= encoded.len() {
            return Err(OutboundError::InvalidCredential);
        }
        let username = std::str::from_utf8(&encoded[2..password_start])
            .map_err(|_| OutboundError::InvalidCredential)?
            .to_owned();
        let password = std::str::from_utf8(&encoded[password_start..])
            .map_err(|_| OutboundError::InvalidCredential)?
            .to_owned();
        Self::new(username, password)
    }

    #[must_use]
    pub fn username(&self) -> &str {
        self.username.as_str()
    }

    #[must_use]
    pub fn password(&self) -> &str {
        self.password.as_str()
    }
}

impl fmt::Debug for ProxyCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProxyCredentials([REDACTED])")
    }
}
