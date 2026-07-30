use crate::StorageError;

pub const MAX_SNAPSHOT_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotStatus {
    Pending,
    Active,
    Superseded,
    Rejected,
}

impl SnapshotStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Rejected => "rejected",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "rejected" => Ok(Self::Rejected),
            _ => Err(StorageError::CorruptData {
                field: "policy_snapshot.status",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotArtifact {
    snapshot_version: u64,
    schema_version: u32,
    created_at_unix_ms: u64,
    content_hash: [u8; 32],
    policy_count: usize,
    payload: Vec<u8>,
}

impl SnapshotArtifact {
    pub fn new(
        snapshot_version: u64,
        schema_version: u32,
        created_at_unix_ms: u64,
        content_hash: [u8; 32],
        policy_count: usize,
        payload: Vec<u8>,
    ) -> Result<Self, StorageError> {
        if snapshot_version == 0
            || schema_version == 0
            || payload.is_empty()
            || payload.len() > MAX_SNAPSHOT_PAYLOAD_BYTES
        {
            return Err(StorageError::SnapshotPayloadInvalid);
        }
        Ok(Self {
            snapshot_version,
            schema_version,
            created_at_unix_ms,
            content_hash,
            policy_count,
            payload,
        })
    }

    #[must_use]
    pub const fn snapshot_version(&self) -> u64 {
        self.snapshot_version
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub const fn created_at_unix_ms(&self) -> u64 {
        self.created_at_unix_ms
    }

    #[must_use]
    pub const fn content_hash(&self) -> &[u8; 32] {
        &self.content_hash
    }

    #[must_use]
    pub const fn policy_count(&self) -> usize {
        self.policy_count
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAckState {
    Loaded,
    Rejected,
}

impl ProviderAckState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Loaded => "loaded",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderAck {
    provider_id: String,
    provider_generation: u64,
    content_hash: [u8; 32],
    state: ProviderAckState,
    error_code: Option<String>,
    acknowledged_at_unix_ms: u64,
}

impl ProviderAck {
    pub fn loaded(
        provider_id: impl Into<String>,
        provider_generation: u64,
        content_hash: [u8; 32],
        acknowledged_at_unix_ms: u64,
    ) -> Result<Self, StorageError> {
        Self::new(
            provider_id,
            provider_generation,
            content_hash,
            ProviderAckState::Loaded,
            None,
            acknowledged_at_unix_ms,
        )
    }

    pub fn rejected(
        provider_id: impl Into<String>,
        provider_generation: u64,
        content_hash: [u8; 32],
        error_code: impl Into<String>,
        acknowledged_at_unix_ms: u64,
    ) -> Result<Self, StorageError> {
        Self::new(
            provider_id,
            provider_generation,
            content_hash,
            ProviderAckState::Rejected,
            Some(error_code.into()),
            acknowledged_at_unix_ms,
        )
    }

    fn new(
        provider_id: impl Into<String>,
        provider_generation: u64,
        content_hash: [u8; 32],
        state: ProviderAckState,
        error_code: Option<String>,
        acknowledged_at_unix_ms: u64,
    ) -> Result<Self, StorageError> {
        let provider_id = provider_id.into();
        validate_provider_id(&provider_id)?;
        if let Some(code) = error_code.as_deref() {
            validate_error_code(code)?;
        }
        Ok(Self {
            provider_id,
            provider_generation,
            content_hash,
            state,
            error_code,
            acknowledged_at_unix_ms,
        })
    }

    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    #[must_use]
    pub const fn provider_generation(&self) -> u64 {
        self.provider_generation
    }

    #[must_use]
    pub const fn content_hash(&self) -> &[u8; 32] {
        &self.content_hash
    }

    #[must_use]
    pub const fn state(&self) -> ProviderAckState {
        self.state
    }

    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }

    #[must_use]
    pub const fn acknowledged_at_unix_ms(&self) -> u64 {
        self.acknowledged_at_unix_ms
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotRecord {
    artifact: SnapshotArtifact,
    source_snapshot_version: Option<u64>,
    status: SnapshotStatus,
    activated_at_unix_ms: Option<u64>,
    failure_code: Option<String>,
}

impl SnapshotRecord {
    pub(crate) fn new(
        artifact: SnapshotArtifact,
        source_snapshot_version: Option<u64>,
        status: SnapshotStatus,
        activated_at_unix_ms: Option<u64>,
        failure_code: Option<String>,
    ) -> Self {
        Self {
            artifact,
            source_snapshot_version,
            status,
            activated_at_unix_ms,
            failure_code,
        }
    }

    #[must_use]
    pub const fn artifact(&self) -> &SnapshotArtifact {
        &self.artifact
    }

    #[must_use]
    pub const fn source_snapshot_version(&self) -> Option<u64> {
        self.source_snapshot_version
    }

    #[must_use]
    pub const fn status(&self) -> SnapshotStatus {
        self.status
    }

    #[must_use]
    pub const fn activated_at_unix_ms(&self) -> Option<u64> {
        self.activated_at_unix_ms
    }

    #[must_use]
    pub fn failure_code(&self) -> Option<&str> {
        self.failure_code.as_deref()
    }
}

pub(crate) fn validate_provider_id(value: &str) -> Result<(), StorageError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(StorageError::ProviderIdInvalid);
    }
    Ok(())
}

pub(crate) fn validate_error_code(value: &str) -> Result<(), StorageError> {
    if !value.starts_with("NP_")
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(StorageError::ErrorCodeInvalid);
    }
    Ok(())
}
