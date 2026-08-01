use nonproxy_model::OutboundId;
use rusqlite::Row;

use crate::{
    CredentialKind, CredentialReference, StorageError, SubscriptionNodeOwnership,
    SubscriptionSource,
};

pub(crate) struct RawSubscriptionSource {
    pub id: String,
    pub display_name: String,
    pub credential_reference: String,
    pub credential_label: String,
    pub credential_version: i64,
    pub enabled: i64,
    pub refresh_interval_seconds: i64,
    pub revision: i64,
    pub content_generation: i64,
    pub consecutive_failures: i64,
    pub next_refresh_at_unix_ms: i64,
    pub last_attempted_at_unix_ms: Option<i64>,
    pub last_succeeded_at_unix_ms: Option<i64>,
    pub last_error_code: Option<String>,
    pub content_hash: Option<Vec<u8>>,
    pub node_count: i64,
}

impl RawSubscriptionSource {
    pub(crate) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            display_name: row.get(1)?,
            credential_reference: row.get(2)?,
            credential_label: row.get(3)?,
            credential_version: row.get(4)?,
            enabled: row.get(5)?,
            refresh_interval_seconds: row.get(6)?,
            revision: row.get(7)?,
            content_generation: row.get(8)?,
            consecutive_failures: row.get(9)?,
            next_refresh_at_unix_ms: row.get(10)?,
            last_attempted_at_unix_ms: row.get(11)?,
            last_succeeded_at_unix_ms: row.get(12)?,
            last_error_code: row.get(13)?,
            content_hash: row.get(14)?,
            node_count: row.get(15)?,
        })
    }
}

pub(crate) fn decode_source(
    raw: RawSubscriptionSource,
) -> Result<SubscriptionSource, StorageError> {
    let credential = CredentialReference::new(
        raw.credential_reference,
        CredentialKind::SubscriptionUrl,
        raw.credential_label,
        as_u64(
            raw.credential_version,
            "subscription.endpoint_credential_version",
        )?,
    )?;
    let enabled = match raw.enabled {
        0 => false,
        1 => true,
        _ => return corrupt("subscription.enabled"),
    };
    let content_hash = raw
        .content_hash
        .map(|value| {
            value.try_into().map_err(|_| StorageError::CorruptData {
                field: "subscription.content_hash",
            })
        })
        .transpose()?;
    SubscriptionSource::from_parts(
        raw.id,
        raw.display_name,
        credential,
        enabled,
        as_u32(
            raw.refresh_interval_seconds,
            "subscription.refresh_interval_seconds",
        )?,
        as_u64(raw.revision, "subscription.revision")?,
        as_u64(raw.content_generation, "subscription.content_generation")?,
        as_u32(
            raw.consecutive_failures,
            "subscription.consecutive_failures",
        )?,
        as_u64(
            raw.next_refresh_at_unix_ms,
            "subscription.next_refresh_at_unix_ms",
        )?,
        optional_u64(
            raw.last_attempted_at_unix_ms,
            "subscription.last_attempted_at_unix_ms",
        )?,
        optional_u64(
            raw.last_succeeded_at_unix_ms,
            "subscription.last_succeeded_at_unix_ms",
        )?,
        raw.last_error_code,
        content_hash,
        as_u32(raw.node_count, "subscription.node_count")?,
    )
}

pub(crate) fn decode_ownership(
    outbound_id: String,
    node_key: String,
    present: i64,
    generation: i64,
) -> Result<SubscriptionNodeOwnership, StorageError> {
    let outbound_id = OutboundId::new(outbound_id)?;
    let present = match present {
        0 => false,
        1 => true,
        _ => return corrupt("subscription_outbound.present"),
    };
    SubscriptionNodeOwnership::stored(
        outbound_id,
        node_key,
        present,
        as_u64(generation, "subscription_outbound.last_seen_generation")?,
    )
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptData { field })
}

fn as_u32(value: i64, field: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::CorruptData { field })
}

fn optional_u64(value: Option<i64>, field: &'static str) -> Result<Option<u64>, StorageError> {
    value.map(|value| as_u64(value, field)).transpose()
}

fn corrupt<T>(field: &'static str) -> Result<T, StorageError> {
    Err(StorageError::CorruptData { field })
}
