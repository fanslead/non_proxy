use std::net::IpAddr;

use nonproxy_model::{DomainName, OutboundId};

use crate::StorageError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundKind {
    HttpConnect,
    Socks5,
    Adapter,
}

impl OutboundKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::HttpConnect => "http_connect",
            Self::Socks5 => "socks5",
            Self::Adapter => "adapter",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "http_connect" => Ok(Self::HttpConnect),
            "socks5" => Ok(Self::Socks5),
            "adapter" => Ok(Self::Adapter),
            _ => Err(StorageError::CorruptData {
                field: "outbound.kind",
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialKind {
    Password,
    BearerToken,
    ClientCertificate,
    PrivateKey,
}

impl CredentialKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::BearerToken => "bearer_token",
            Self::ClientCertificate => "client_certificate",
            Self::PrivateKey => "private_key",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "password" => Ok(Self::Password),
            "bearer_token" => Ok(Self::BearerToken),
            "client_certificate" => Ok(Self::ClientCertificate),
            "private_key" => Ok(Self::PrivateKey),
            _ => Err(StorageError::CorruptData {
                field: "outbound.credential_kind",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialReference {
    item_reference: String,
    kind: CredentialKind,
    display_label: String,
    version: u64,
}

impl CredentialReference {
    pub fn new(
        item_reference: impl Into<String>,
        kind: CredentialKind,
        display_label: impl Into<String>,
        version: u64,
    ) -> Result<Self, StorageError> {
        let item_reference = item_reference.into();
        let display_label = display_label.into();
        if item_reference.is_empty()
            || item_reference.len() > 512
            || !item_reference.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
            || display_label.is_empty()
            || display_label.len() > 128
            || display_label.chars().any(char::is_control)
            || version == 0
        {
            return Err(StorageError::CredentialReferenceInvalid);
        }
        Ok(Self {
            item_reference,
            kind,
            display_label,
            version,
        })
    }

    #[must_use]
    pub fn item_reference(&self) -> &str {
        &self.item_reference
    }

    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }

    #[must_use]
    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundReference {
    id: OutboundId,
    kind: OutboundKind,
    endpoint_host: Option<String>,
    endpoint_port: Option<u16>,
    credential: Option<CredentialReference>,
    enabled: bool,
    revision: u64,
}

impl OutboundReference {
    pub fn new(
        id: OutboundId,
        kind: OutboundKind,
        endpoint_host: Option<&str>,
        endpoint_port: Option<u16>,
        credential: Option<CredentialReference>,
        revision: u64,
    ) -> Result<Self, StorageError> {
        let endpoint_host = endpoint_host.map(normalize_endpoint).transpose()?;
        let endpoint_shape_valid = match kind {
            OutboundKind::HttpConnect | OutboundKind::Socks5 => {
                endpoint_host.is_some() && endpoint_port.is_some_and(|port| port > 0)
            }
            OutboundKind::Adapter => endpoint_host.is_none() && endpoint_port.is_none(),
        };
        if !endpoint_shape_valid || revision == 0 {
            return Err(StorageError::OutboundInvalid);
        }
        Ok(Self {
            id,
            kind,
            endpoint_host,
            endpoint_port,
            credential,
            enabled: true,
            revision,
        })
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    #[must_use]
    pub const fn id(&self) -> &OutboundId {
        &self.id
    }

    #[must_use]
    pub const fn kind(&self) -> OutboundKind {
        self.kind
    }

    #[must_use]
    pub fn endpoint_host(&self) -> Option<&str> {
        self.endpoint_host.as_deref()
    }

    #[must_use]
    pub const fn endpoint_port(&self) -> Option<u16> {
        self.endpoint_port
    }

    #[must_use]
    pub const fn credential(&self) -> Option<&CredentialReference> {
        self.credential.as_ref()
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

fn normalize_endpoint(value: &str) -> Result<String, StorageError> {
    if value.is_empty()
        || value.trim() != value
        || value.contains("://")
        || value
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@'))
    {
        return Err(StorageError::OutboundInvalid);
    }
    if let Ok(address) = value.parse::<IpAddr>() {
        return Ok(address.to_string());
    }
    DomainName::normalize(value)
        .map(|domain| domain.as_ascii().to_owned())
        .map_err(StorageError::from)
}
