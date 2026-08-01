use nonproxy_model::OutboundId;

use crate::{CredentialKind, CredentialReference, OutboundReference, StorageError};

pub const MINIMUM_REFRESH_INTERVAL_SECONDS: u32 = 15 * 60;
pub const MAXIMUM_REFRESH_INTERVAL_SECONDS: u32 = 7 * 24 * 60 * 60;
const MAXIMUM_SUBSCRIPTION_ID_BYTES: usize = 64;
const MAXIMUM_DISPLAY_NAME_BYTES: usize = 128;
const MAXIMUM_NODE_KEY_BYTES: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionSource {
    id: String,
    display_name: String,
    endpoint_credential: CredentialReference,
    enabled: bool,
    refresh_interval_seconds: u32,
    revision: u64,
    content_generation: u64,
    consecutive_failures: u32,
    next_refresh_at_unix_ms: u64,
    last_attempted_at_unix_ms: Option<u64>,
    last_succeeded_at_unix_ms: Option<u64>,
    last_error_code: Option<String>,
    content_hash: Option<[u8; 32]>,
    node_count: u32,
}

impl SubscriptionSource {
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        endpoint_credential: CredentialReference,
        refresh_interval_seconds: u32,
        revision: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<Self, StorageError> {
        Self::from_parts(
            id.into(),
            display_name.into(),
            endpoint_credential,
            true,
            refresh_interval_seconds,
            revision,
            0,
            0,
            next_refresh_at_unix_ms,
            None,
            None,
            None,
            None,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        id: String,
        display_name: String,
        endpoint_credential: CredentialReference,
        enabled: bool,
        refresh_interval_seconds: u32,
        revision: u64,
        content_generation: u64,
        consecutive_failures: u32,
        next_refresh_at_unix_ms: u64,
        last_attempted_at_unix_ms: Option<u64>,
        last_succeeded_at_unix_ms: Option<u64>,
        last_error_code: Option<String>,
        content_hash: Option<[u8; 32]>,
        node_count: u32,
    ) -> Result<Self, StorageError> {
        validate_identifier(&id, MAXIMUM_SUBSCRIPTION_ID_BYTES)?;
        validate_display_name(&display_name)?;
        if endpoint_credential.kind() != CredentialKind::SubscriptionUrl
            || !(MINIMUM_REFRESH_INTERVAL_SECONDS..=MAXIMUM_REFRESH_INTERVAL_SECONDS)
                .contains(&refresh_interval_seconds)
            || revision == 0
            || node_count > 100
            || !valid_refresh_state(
                content_generation,
                last_attempted_at_unix_ms,
                last_succeeded_at_unix_ms,
                last_error_code.as_deref(),
                content_hash,
                node_count,
            )
        {
            return Err(StorageError::SubscriptionInvalid);
        }
        Ok(Self {
            id,
            display_name,
            endpoint_credential,
            enabled,
            refresh_interval_seconds,
            revision,
            content_generation,
            consecutive_failures,
            next_refresh_at_unix_ms,
            last_attempted_at_unix_ms,
            last_succeeded_at_unix_ms,
            last_error_code,
            content_hash,
            node_count,
        })
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn reconfigured(
        &self,
        display_name: impl Into<String>,
        endpoint_credential: CredentialReference,
        enabled: bool,
        refresh_interval_seconds: u32,
        revision: u64,
    ) -> Result<Self, StorageError> {
        Self::from_parts(
            self.id.clone(),
            display_name.into(),
            endpoint_credential,
            enabled,
            refresh_interval_seconds,
            revision,
            self.content_generation,
            self.consecutive_failures,
            self.next_refresh_at_unix_ms,
            self.last_attempted_at_unix_ms,
            self.last_succeeded_at_unix_ms,
            self.last_error_code.clone(),
            self.content_hash,
            self.node_count,
        )
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    #[must_use]
    pub const fn endpoint_credential(&self) -> &CredentialReference {
        &self.endpoint_credential
    }
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    #[must_use]
    pub const fn refresh_interval_seconds(&self) -> u32 {
        self.refresh_interval_seconds
    }
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[must_use]
    pub const fn content_generation(&self) -> u64 {
        self.content_generation
    }
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
    #[must_use]
    pub const fn next_refresh_at_unix_ms(&self) -> u64 {
        self.next_refresh_at_unix_ms
    }
    #[must_use]
    pub const fn last_attempted_at_unix_ms(&self) -> Option<u64> {
        self.last_attempted_at_unix_ms
    }
    #[must_use]
    pub const fn last_succeeded_at_unix_ms(&self) -> Option<u64> {
        self.last_succeeded_at_unix_ms
    }
    #[must_use]
    pub fn last_error_code(&self) -> Option<&str> {
        self.last_error_code.as_deref()
    }
    #[must_use]
    pub const fn content_hash(&self) -> Option<[u8; 32]> {
        self.content_hash
    }
    #[must_use]
    pub const fn node_count(&self) -> u32 {
        self.node_count
    }
}

pub struct SubscriptionNode {
    node_key: String,
    outbound: OutboundReference,
    expected_revision: Option<u64>,
}

impl SubscriptionNode {
    pub fn new(
        node_key: impl Into<String>,
        outbound: OutboundReference,
        expected_revision: Option<u64>,
    ) -> Result<Self, StorageError> {
        let node_key = node_key.into();
        validate_identifier(&node_key, MAXIMUM_NODE_KEY_BYTES)?;
        Ok(Self {
            node_key,
            outbound,
            expected_revision,
        })
    }

    #[must_use]
    pub fn node_key(&self) -> &str {
        &self.node_key
    }
    #[must_use]
    pub const fn outbound(&self) -> &OutboundReference {
        &self.outbound
    }
    #[must_use]
    pub const fn expected_revision(&self) -> Option<u64> {
        self.expected_revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionNodeOwnership {
    outbound_id: OutboundId,
    node_key: String,
    present: bool,
    last_seen_generation: u64,
}

impl SubscriptionNodeOwnership {
    #[must_use]
    pub const fn outbound_id(&self) -> &OutboundId {
        &self.outbound_id
    }
    #[must_use]
    pub fn node_key(&self) -> &str {
        &self.node_key
    }
    #[must_use]
    pub const fn present(&self) -> bool {
        self.present
    }
    #[must_use]
    pub const fn last_seen_generation(&self) -> u64 {
        self.last_seen_generation
    }

    pub(crate) fn stored(
        outbound_id: OutboundId,
        node_key: String,
        present: bool,
        generation: u64,
    ) -> Result<Self, StorageError> {
        validate_identifier(&node_key, MAXIMUM_NODE_KEY_BYTES)?;
        if generation == 0 {
            return Err(StorageError::SubscriptionInvalid);
        }
        Ok(Self {
            outbound_id,
            node_key,
            present,
            last_seen_generation: generation,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct SubscriptionRefreshCommit {
    generation: u64,
    replaced_credential_references: Vec<String>,
    retired_outbound_ids: Vec<OutboundId>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SubscriptionDeleteCommit {
    credential_references: Vec<String>,
    outbound_count: usize,
}

impl SubscriptionDeleteCommit {
    #[must_use]
    pub fn credential_references(&self) -> &[String] {
        &self.credential_references
    }

    #[must_use]
    pub const fn outbound_count(&self) -> usize {
        self.outbound_count
    }

    pub(crate) fn new(mut credential_references: Vec<String>, outbound_count: usize) -> Self {
        credential_references.sort();
        credential_references.dedup();
        Self {
            credential_references,
            outbound_count,
        }
    }
}

impl SubscriptionRefreshCommit {
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    #[must_use]
    pub fn replaced_credential_references(&self) -> &[String] {
        &self.replaced_credential_references
    }
    #[must_use]
    pub fn retired_outbound_ids(&self) -> &[OutboundId] {
        &self.retired_outbound_ids
    }

    pub(crate) fn new(generation: u64, replaced: Vec<String>, retired: Vec<OutboundId>) -> Self {
        Self {
            generation,
            replaced_credential_references: replaced,
            retired_outbound_ids: retired,
        }
    }

    pub(crate) fn normalize_replaced_credentials(&mut self) {
        self.replaced_credential_references.sort();
        self.replaced_credential_references.dedup();
    }

    pub(crate) fn add_replaced_credential(&mut self, reference: String) {
        self.replaced_credential_references.push(reference);
    }
}

fn validate_identifier(value: &str, maximum: usize) -> Result<(), StorageError> {
    if value.is_empty()
        || value.len() > maximum
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(StorageError::SubscriptionInvalid);
    }
    Ok(())
}

fn validate_display_name(value: &str) -> Result<(), StorageError> {
    if value.is_empty()
        || value.len() > MAXIMUM_DISPLAY_NAME_BYTES
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(StorageError::SubscriptionInvalid);
    }
    Ok(())
}

fn valid_refresh_state(
    generation: u64,
    attempted: Option<u64>,
    succeeded: Option<u64>,
    error: Option<&str>,
    hash: Option<[u8; 32]>,
    node_count: u32,
) -> bool {
    let error_valid = error.is_none_or(|value| {
        attempted.is_some()
            && value.starts_with("NP_")
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    });
    let time_valid =
        succeeded.is_none_or(|value| attempted.is_some_and(|attempt| attempt >= value));
    let content_valid = if generation == 0 {
        succeeded.is_none() && hash.is_none() && node_count == 0
    } else {
        succeeded.is_some() && hash.is_some() && node_count > 0
    };
    error_valid && time_valid && content_valid
}
