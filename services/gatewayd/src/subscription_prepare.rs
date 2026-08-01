use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use nonproxy_model::{DomainName, OutboundId};
use nonproxy_outbound::ShadowsocksCredentials;
use nonproxy_storage::{
    CredentialKind, CredentialReference, OutboundKind, OutboundReference, SubscriptionNode,
    SubscriptionNodeOwnership,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    credential_store::CredentialWrite,
    outbound_import::{OutboundImportError, RawOutbound, RawOutboundKind},
};

pub(crate) struct PreparedSubscriptionRefresh {
    pub(crate) nodes: Vec<SubscriptionNode>,
    pub(crate) credentials: Vec<CredentialWrite>,
}

pub(crate) fn prepare_subscription_refresh(
    source_id: &str,
    payload: &[u8],
    refresh_id: &str,
    ownership: &[SubscriptionNodeOwnership],
    current_outbounds: &[OutboundReference],
) -> Result<PreparedSubscriptionRefresh, SubscriptionPrepareError> {
    let raw_nodes = crate::outbound_subscription::parse(payload)?;
    let owned = ownership
        .iter()
        .map(|value| (value.node_key(), value))
        .collect::<HashMap<_, _>>();
    let current = current_outbounds
        .iter()
        .map(|value| (value.id().as_str(), value))
        .collect::<HashMap<_, _>>();
    let mut node_keys = HashSet::new();
    let mut nodes = Vec::with_capacity(raw_nodes.len());
    let mut credentials = Vec::with_capacity(raw_nodes.len());

    for raw in raw_nodes {
        let prepared = prepare_node(source_id, refresh_id, raw, &owned, &current, &mut node_keys)?;
        credentials.push(prepared.credential);
        nodes.push(prepared.node);
    }
    Ok(PreparedSubscriptionRefresh { nodes, credentials })
}

struct PreparedNode {
    node: SubscriptionNode,
    credential: CredentialWrite,
}

fn prepare_node(
    source_id: &str,
    refresh_id: &str,
    raw: RawOutbound,
    owned: &HashMap<&str, &SubscriptionNodeOwnership>,
    current: &HashMap<&str, &OutboundReference>,
    node_keys: &mut HashSet<String>,
) -> Result<PreparedNode, SubscriptionPrepareError> {
    if !matches!(raw.kind, RawOutboundKind::Shadowsocks) || raw.username.is_some() || !raw.enabled {
        return Err(SubscriptionPrepareError::ContentInvalid);
    }
    let method = raw.method.ok_or(SubscriptionPrepareError::ContentInvalid)?;
    let password = raw
        .password
        .ok_or(SubscriptionPrepareError::ContentInvalid)?;
    let credential = ShadowsocksCredentials::new(&method, password)
        .map_err(|_| SubscriptionPrepareError::ContentInvalid)?;
    let host = normalize_host(&raw.host)?;
    let method = credential.method_name();
    let node_key = node_key(&host, raw.port, &method);
    if !node_keys.insert(node_key.clone()) {
        return Err(SubscriptionPrepareError::DuplicateNode);
    }

    let existing_owner = owned.get(node_key.as_str()).copied();
    let deterministic_id = outbound_id(source_id, &node_key)?;
    let outbound_id = existing_owner
        .map(|value| value.outbound_id().clone())
        .unwrap_or(deterministic_id);
    let existing = current.get(outbound_id.as_str()).copied();
    if (existing_owner.is_some() && existing.is_none())
        || (existing_owner.is_none() && existing.is_some())
    {
        return Err(SubscriptionPrepareError::OwnershipStateInvalid);
    }
    let expected_revision = existing.map(OutboundReference::revision);
    let revision = expected_revision
        .map_or(Some(1), |value| value.checked_add(1))
        .ok_or(SubscriptionPrepareError::RevisionExhausted)?;
    let reference = format!("outbound:{}:v{revision}:{refresh_id}", outbound_id.as_str());
    let metadata = CredentialReference::new(
        &reference,
        CredentialKind::Password,
        format!("{} Shadowsocks 密钥", outbound_id.as_str()),
        revision,
    )
    .map_err(|_| SubscriptionPrepareError::ContentInvalid)?;
    let outbound = OutboundReference::new(
        outbound_id,
        OutboundKind::Shadowsocks,
        Some(&host),
        Some(raw.port),
        Some(metadata),
        revision,
    )
    .map_err(|_| SubscriptionPrepareError::ContentInvalid)?;
    let node = SubscriptionNode::new(node_key, outbound, expected_revision)
        .map_err(|_| SubscriptionPrepareError::ContentInvalid)?;
    Ok(PreparedNode {
        node,
        credential: CredentialWrite {
            reference,
            secret: credential.encode(),
        },
    })
}

fn normalize_host(value: &str) -> Result<String, SubscriptionPrepareError> {
    if let Ok(address) = value.parse::<IpAddr>() {
        return Ok(address.to_string());
    }
    DomainName::normalize(value)
        .map(|domain| domain.as_ascii().to_owned())
        .map_err(|_| SubscriptionPrepareError::ContentInvalid)
}

fn node_key(host: &str, port: u16, method: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"nonproxy-subscription-node-v1\0");
    digest.update(host.as_bytes());
    digest.update(b"\0");
    digest.update(port.to_be_bytes());
    digest.update(b"\0");
    digest.update(method.as_bytes());
    hex(&digest.finalize())
}

fn outbound_id(source_id: &str, node_key: &str) -> Result<OutboundId, SubscriptionPrepareError> {
    let source_hash = Sha256::digest(source_id.as_bytes());
    OutboundId::new(format!(
        "sub:{}:{}",
        &hex(&source_hash)[..16],
        &node_key[..48]
    ))
    .map_err(|_| SubscriptionPrepareError::ContentInvalid)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug, Error)]
pub(crate) enum SubscriptionPrepareError {
    #[error("订阅内容无效或包含不支持的节点")]
    ContentInvalid,
    #[error("订阅内容包含重复节点")]
    DuplicateNode,
    #[error("订阅节点修订号已耗尽")]
    RevisionExhausted,
    #[error("订阅节点归属状态不一致")]
    OwnershipStateInvalid,
}

impl From<OutboundImportError> for SubscriptionPrepareError {
    fn from(error: OutboundImportError) -> Self {
        match error {
            OutboundImportError::RevisionExhausted => Self::RevisionExhausted,
            _ => Self::ContentInvalid,
        }
    }
}

impl SubscriptionPrepareError {
    #[must_use]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::ContentInvalid => "NP_SUBSCRIPTION_CONTENT_INVALID",
            Self::DuplicateNode => "NP_SUBSCRIPTION_NODE_DUPLICATE",
            Self::RevisionExhausted => "NP_SUBSCRIPTION_NODE_REVISION_EXHAUSTED",
            Self::OwnershipStateInvalid => "NP_SUBSCRIPTION_OWNERSHIP_STATE_INVALID",
        }
    }
}
