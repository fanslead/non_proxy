use std::collections::{HashMap, HashSet};

use nonproxy_model::OutboundId;
use nonproxy_outbound::ShadowsocksCredentials;
use nonproxy_storage::{CredentialKind, CredentialReference, OutboundKind, OutboundReference};
use serde::Deserialize;
use thiserror::Error;
use zeroize::Zeroizing;

pub const IMPORT_FORMAT: &str = "nonproxy-json-v1";
pub const URI_LIST_IMPORT_FORMAT: &str = "proxy-uri-list-v1";
pub const MAX_IMPORT_BYTES: usize = 256 * 1024;
const MAX_OUTBOUNDS: usize = 100;
const MAX_CREDENTIAL_FIELD_BYTES: usize = 255;

pub struct PreparedImport {
    pub import_id: String,
    pub outbounds: Vec<(OutboundReference, Option<u64>)>,
    pub credentials: Vec<PreparedCredential>,
    pub replaced_credential_references: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct PreparedCredential {
    pub reference: String,
    pub secret: Zeroizing<Vec<u8>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportDocument {
    version: u32,
    outbounds: Vec<RawOutbound>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOutbound {
    pub id: String,
    pub kind: RawOutboundKind,
    pub host: String,
    pub port: u16,
    pub method: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawOutboundKind {
    HttpConnect,
    Socks5,
    Shadowsocks,
}

struct RawCredential {
    kind: RawOutboundKind,
    method: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

pub fn prepare(
    format: &str,
    configuration: &[u8],
    import_id: String,
    current: &[OutboundReference],
) -> Result<PreparedImport, OutboundImportError> {
    if configuration.is_empty() || configuration.len() > MAX_IMPORT_BYTES {
        return Err(OutboundImportError::Invalid);
    }
    let raw_outbounds = match format {
        IMPORT_FORMAT => parse_json(configuration)?,
        URI_LIST_IMPORT_FORMAT => crate::outbound_import_uri::parse(configuration)?,
        _ => return Err(OutboundImportError::Invalid),
    };
    let existing = current
        .iter()
        .map(|value| (value.id().as_str(), value))
        .collect::<HashMap<_, _>>();
    let mut identifiers = HashSet::new();
    let mut outbounds = Vec::with_capacity(raw_outbounds.len());
    let mut credentials = Vec::new();
    let mut replaced = Vec::new();
    let mut warnings = Vec::new();

    for raw in raw_outbounds {
        if !identifiers.insert(raw.id.clone()) {
            return Err(OutboundImportError::DuplicateId);
        }
        let id = OutboundId::new(raw.id).map_err(|_| OutboundImportError::Invalid)?;
        let current = existing.get(id.as_str()).copied();
        if current.is_some() {
            warnings.push(format!("出口 {} 已存在，保存时将安全更新。", id.as_str()));
        }
        let expected_revision = current.map(OutboundReference::revision);
        let revision = expected_revision
            .map_or(Some(1), |value| value.checked_add(1))
            .ok_or(OutboundImportError::RevisionExhausted)?;
        let credential = prepare_credential(
            &id,
            revision,
            &import_id,
            RawCredential {
                kind: raw.kind,
                method: raw.method,
                username: raw.username,
                password: raw.password,
            },
            &mut credentials,
        )?;
        if let Some(reference) = current
            .and_then(OutboundReference::credential)
            .map(CredentialReference::item_reference)
        {
            replaced.push(reference.to_owned());
        }
        let kind = match raw.kind {
            RawOutboundKind::HttpConnect => {
                warnings.push(format!("出口 {} 仅支持 TCP。", id.as_str()));
                OutboundKind::HttpConnect
            }
            RawOutboundKind::Socks5 => OutboundKind::Socks5,
            RawOutboundKind::Shadowsocks => OutboundKind::Shadowsocks,
        };
        let mut outbound = OutboundReference::new(
            id,
            kind,
            Some(&raw.host),
            Some(raw.port),
            credential,
            revision,
        )
        .map_err(|_| OutboundImportError::Invalid)?;
        if !raw.enabled {
            outbound = outbound.disabled();
        }
        outbounds.push((outbound, expected_revision));
    }

    Ok(PreparedImport {
        import_id,
        outbounds,
        credentials,
        replaced_credential_references: replaced,
        warnings,
    })
}

fn parse_json(configuration: &[u8]) -> Result<Vec<RawOutbound>, OutboundImportError> {
    let document: ImportDocument =
        serde_json::from_slice(configuration).map_err(|_| OutboundImportError::Invalid)?;
    if document.version != 1
        || document.outbounds.is_empty()
        || document.outbounds.len() > MAX_OUTBOUNDS
    {
        return Err(OutboundImportError::Invalid);
    }
    Ok(document.outbounds)
}

fn prepare_credential(
    id: &OutboundId,
    revision: u64,
    import_id: &str,
    raw: RawCredential,
    credentials: &mut Vec<PreparedCredential>,
) -> Result<Option<CredentialReference>, OutboundImportError> {
    match raw.kind {
        RawOutboundKind::Shadowsocks => prepare_shadowsocks_credential(
            id,
            revision,
            import_id,
            raw.method,
            raw.username,
            raw.password,
            credentials,
        ),
        RawOutboundKind::HttpConnect | RawOutboundKind::Socks5 => {
            if raw.method.is_some() {
                return Err(OutboundImportError::Invalid);
            }
            prepare_proxy_credential(
                id,
                revision,
                import_id,
                raw.username,
                raw.password,
                credentials,
            )
        }
    }
}

fn prepare_proxy_credential(
    id: &OutboundId,
    revision: u64,
    import_id: &str,
    username: Option<String>,
    password: Option<String>,
    credentials: &mut Vec<PreparedCredential>,
) -> Result<Option<CredentialReference>, OutboundImportError> {
    let (username, password) = match (username, password) {
        (None, None) => return Ok(None),
        (Some(username), Some(password)) => (username, Zeroizing::new(password)),
        _ => return Err(OutboundImportError::CredentialPair),
    };
    if username.is_empty()
        || password.is_empty()
        || username.len() > MAX_CREDENTIAL_FIELD_BYTES
        || password.len() > MAX_CREDENTIAL_FIELD_BYTES
        || username.chars().any(char::is_control)
        || password.chars().any(char::is_control)
    {
        return Err(OutboundImportError::CredentialInvalid);
    }
    let (reference, metadata) = credential_reference(id, revision, import_id, "代理凭据")?;
    let mut secret = Zeroizing::new(Vec::with_capacity(2 + username.len() + password.len()));
    secret.push(1);
    secret.push(u8::try_from(username.len()).map_err(|_| OutboundImportError::CredentialInvalid)?);
    secret.extend_from_slice(username.as_bytes());
    secret.extend_from_slice(password.as_bytes());
    credentials.push(PreparedCredential { reference, secret });
    Ok(Some(metadata))
}

fn prepare_shadowsocks_credential(
    id: &OutboundId,
    revision: u64,
    import_id: &str,
    method: Option<String>,
    username: Option<String>,
    password: Option<String>,
    credentials: &mut Vec<PreparedCredential>,
) -> Result<Option<CredentialReference>, OutboundImportError> {
    let (Some(method), None, Some(password)) = (method, username, password) else {
        return Err(OutboundImportError::ShadowsocksCredential);
    };
    let credential = ShadowsocksCredentials::new(&method, password)
        .map_err(|_| OutboundImportError::ShadowsocksCredential)?;
    let (reference, metadata) = credential_reference(id, revision, import_id, "Shadowsocks 密钥")?;
    credentials.push(PreparedCredential {
        reference,
        secret: credential.encode(),
    });
    Ok(Some(metadata))
}

fn credential_reference(
    id: &OutboundId,
    revision: u64,
    import_id: &str,
    label: &str,
) -> Result<(String, CredentialReference), OutboundImportError> {
    let reference = format!("outbound:{}:v{revision}:{import_id}", id.as_str());
    let metadata = CredentialReference::new(
        &reference,
        CredentialKind::Password,
        format!("{} {label}", id.as_str()),
        revision,
    )
    .map_err(|_| OutboundImportError::CredentialInvalid)?;
    Ok((reference, metadata))
}

const fn enabled_by_default() -> bool {
    true
}

#[derive(Debug, Error)]
pub enum OutboundImportError {
    #[error("导入格式或字段无效")]
    Invalid,
    #[error("同一次导入包含重复出口标识")]
    DuplicateId,
    #[error("用户名和密码必须同时提供")]
    CredentialPair,
    #[error("代理凭据为空或超出协议长度")]
    CredentialInvalid,
    #[error("Shadowsocks 必须提供受支持的 AEAD 加密方法和有效密钥")]
    ShadowsocksCredential,
    #[error("出口配置修订号已耗尽")]
    RevisionExhausted,
    #[error("第 {line} 行不是受支持的 SOCKS5、HTTP 或 Shadowsocks 代理链接")]
    UriInvalid { line: usize },
    #[error("第 {line} 行的代理链接协议尚不支持")]
    UriSchemeUnsupported { line: usize },
    #[error("第 {line} 行的账号或密码编码无效")]
    UriCredentialInvalid { line: usize },
}

impl OutboundImportError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Invalid => "NP_OUTBOUND_IMPORT_INVALID",
            Self::DuplicateId => "NP_OUTBOUND_IMPORT_DUPLICATE_ID",
            Self::CredentialPair | Self::CredentialInvalid | Self::ShadowsocksCredential => {
                "NP_OUTBOUND_CREDENTIAL_INVALID"
            }
            Self::RevisionExhausted => "NP_OUTBOUND_REVISION_EXHAUSTED",
            Self::UriInvalid { .. } => "NP_OUTBOUND_IMPORT_URI_INVALID",
            Self::UriSchemeUnsupported { .. } => "NP_OUTBOUND_IMPORT_URI_SCHEME_UNSUPPORTED",
            Self::UriCredentialInvalid { .. } => "NP_OUTBOUND_IMPORT_URI_CREDENTIAL_INVALID",
        }
    }

    #[must_use]
    pub const fn line(&self) -> Option<usize> {
        match self {
            Self::UriInvalid { line }
            | Self::UriSchemeUnsupported { line }
            | Self::UriCredentialInvalid { line } => Some(*line),
            _ => None,
        }
    }
}
