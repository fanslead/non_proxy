use std::{fmt, str::FromStr};

use shadowsocks::{ServerConfig, crypto::CipherKind};
use zeroize::Zeroizing;

use crate::OutboundError;

const CREDENTIAL_VERSION: u8 = 1;
const MAXIMUM_FIELD_BYTES: usize = 255;

pub struct ShadowsocksCredentials {
    method: CipherKind,
    password: Zeroizing<String>,
}

impl ShadowsocksCredentials {
    pub fn new(method: &str, password: String) -> Result<Self, OutboundError> {
        if method.is_empty()
            || method.len() > MAXIMUM_FIELD_BYTES
            || password.is_empty()
            || password.len() > MAXIMUM_FIELD_BYTES
            || method.chars().any(char::is_control)
            || password.chars().any(char::is_control)
        {
            return Err(OutboundError::ShadowsocksCredentialInvalid);
        }
        let method = CipherKind::from_str(method)
            .map_err(|_| OutboundError::ShadowsocksCredentialInvalid)?;
        if !supported_method(method) {
            return Err(OutboundError::ShadowsocksCredentialInvalid);
        }
        ServerConfig::new(("127.0.0.1", 1), password.clone(), method)
            .map_err(|_| OutboundError::ShadowsocksCredentialInvalid)?;
        Ok(Self {
            method,
            password: Zeroizing::new(password),
        })
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, OutboundError> {
        if encoded.len() < 4 || encoded[0] != CREDENTIAL_VERSION {
            return Err(OutboundError::ShadowsocksCredentialInvalid);
        }
        let method_length = usize::from(encoded[1]);
        let password_start = 2_usize
            .checked_add(method_length)
            .ok_or(OutboundError::ShadowsocksCredentialInvalid)?;
        if method_length == 0 || password_start >= encoded.len() {
            return Err(OutboundError::ShadowsocksCredentialInvalid);
        }
        let method = std::str::from_utf8(&encoded[2..password_start])
            .map_err(|_| OutboundError::ShadowsocksCredentialInvalid)?;
        let password = std::str::from_utf8(&encoded[password_start..])
            .map_err(|_| OutboundError::ShadowsocksCredentialInvalid)?
            .to_owned();
        Self::new(method, password)
    }

    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let method_name = self.method_name();
        let method = method_name.as_bytes();
        let mut encoded =
            Zeroizing::new(Vec::with_capacity(2 + method.len() + self.password.len()));
        encoded.push(CREDENTIAL_VERSION);
        encoded.push(method.len() as u8);
        encoded.extend_from_slice(method);
        encoded.extend_from_slice(self.password.as_bytes());
        encoded
    }

    #[must_use]
    pub fn method_name(&self) -> String {
        self.method.to_string()
    }

    pub(crate) fn server_config(
        &self,
        host: &str,
        port: u16,
    ) -> Result<ServerConfig, OutboundError> {
        ServerConfig::new(
            (host.to_owned(), port),
            self.password.to_string(),
            self.method,
        )
        .map_err(|_| OutboundError::ShadowsocksCredentialInvalid)
    }
}

impl fmt::Debug for ShadowsocksCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShadowsocksCredentials")
            .field("method", &self.method_name())
            .field("password", &"[REDACTED]")
            .finish()
    }
}

const fn supported_method(method: CipherKind) -> bool {
    matches!(
        method,
        CipherKind::AES_128_GCM
            | CipherKind::AES_256_GCM
            | CipherKind::CHACHA20_POLY1305
            | CipherKind::AEAD2022_BLAKE3_AES_128_GCM
            | CipherKind::AEAD2022_BLAKE3_AES_256_GCM
            | CipherKind::AEAD2022_BLAKE3_CHACHA20_POLY1305
    )
}
