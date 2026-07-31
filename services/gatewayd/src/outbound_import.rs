use std::collections::{HashMap, HashSet};

use nonproxy_model::OutboundId;
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
            raw.username,
            raw.password,
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
    let reference = format!("outbound:{}:v{revision}:{import_id}", id.as_str());
    let metadata = CredentialReference::new(
        &reference,
        CredentialKind::Password,
        format!("{} 代理凭据", id.as_str()),
        revision,
    )
    .map_err(|_| OutboundImportError::CredentialInvalid)?;
    let mut secret = Zeroizing::new(Vec::with_capacity(2 + username.len() + password.len()));
    secret.push(1);
    secret.push(u8::try_from(username.len()).map_err(|_| OutboundImportError::CredentialInvalid)?);
    secret.extend_from_slice(username.as_bytes());
    secret.extend_from_slice(password.as_bytes());
    credentials.push(PreparedCredential { reference, secret });
    Ok(Some(metadata))
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
    #[error("出口配置修订号已耗尽")]
    RevisionExhausted,
    #[error("第 {line} 行不是受支持的 SOCKS5 或 HTTP 代理链接")]
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
            Self::CredentialPair | Self::CredentialInvalid => "NP_OUTBOUND_CREDENTIAL_INVALID",
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

#[cfg(test)]
mod tests {
    use nonproxy_model::OutboundId;
    use nonproxy_storage::{OutboundKind, OutboundReference};

    use super::{IMPORT_FORMAT, URI_LIST_IMPORT_FORMAT, prepare};

    #[test]
    fn prepares_versioned_credential_without_storing_secret_in_metadata() {
        let configuration = br#"{
            "version": 1,
            "outbounds": [{
                "id": "primary",
                "kind": "socks5",
                "host": "Proxy.Example.com.",
                "port": 1080,
                "username": "alice",
                "password": "private"
            }]
        }"#;

        let prepared = match prepare(
            IMPORT_FORMAT,
            configuration,
            "00112233445566778899aabbccddeeff".to_owned(),
            &[],
        ) {
            Ok(value) => value,
            Err(error) => panic!("出口导入准备失败: {error}"),
        };

        assert_eq!(prepared.outbounds.len(), 1);
        assert_eq!(prepared.credentials.len(), 1);
        let outbound = &prepared.outbounds[0].0;
        assert_eq!(outbound.endpoint_host(), Some("proxy.example.com"));
        let reference = outbound
            .credential()
            .map(nonproxy_storage::CredentialReference::item_reference);
        assert!(reference.is_some_and(|value| !value.contains("private")));
        assert_eq!(
            prepared.credentials[0].secret.as_slice(),
            b"\x01\x05aliceprivate"
        );
    }

    #[test]
    fn rejects_unknown_fields_duplicate_ids_and_partial_credentials() {
        let cases: [&[u8]; 3] = [
            br#"{"version":1,"extra":true,"outbounds":[]}"#,
            br#"{"version":1,"outbounds":[
                {"id":"same","kind":"socks5","host":"a.example","port":1},
                {"id":"same","kind":"socks5","host":"b.example","port":2}
            ]}"#,
            br#"{"version":1,"outbounds":[{
                "id":"partial","kind":"http_connect",
                "host":"a.example","port":8080,"username":"alice"
            }]}"#,
        ];

        for configuration in cases {
            assert!(
                prepare(
                    IMPORT_FORMAT,
                    configuration,
                    "00112233445566778899aabbccddeeff".to_owned(),
                    &[],
                )
                .is_err()
            );
        }
    }

    #[test]
    fn uri_preview_warns_before_replacing_an_existing_identifier() {
        let existing = OutboundReference::new(
            OutboundId::new("office")
                .unwrap_or_else(|error| panic!("现有出口标识创建失败: {error}")),
            OutboundKind::Socks5,
            Some("old.example"),
            Some(1_080),
            None,
            7,
        )
        .unwrap_or_else(|error| panic!("现有出口配置创建失败: {error}"));

        let prepared = prepare(
            URI_LIST_IMPORT_FORMAT,
            b"socks5://new.example:1080#office",
            "00112233445566778899aabbccddeeff".to_owned(),
            &[existing],
        )
        .unwrap_or_else(|error| panic!("替换预检失败: {error}"));

        assert_eq!(prepared.outbounds[0].1, Some(7));
        assert_eq!(prepared.outbounds[0].0.revision(), 8);
        assert!(
            prepared
                .warnings
                .iter()
                .any(|value| value.contains("office 已存在") && value.contains("安全更新"))
        );
    }
}
