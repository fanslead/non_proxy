use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    StorageError, SubscriptionRepository, migration::to_sqlite_u64,
    subscription_repository::classify_source_conflict,
};

pub(crate) fn validate_source_state(
    transaction: &Transaction<'_>,
    source_id: &str,
    expected_revision: u64,
    expected_generation: u64,
) -> Result<u64, StorageError> {
    let current = transaction
        .query_row(
            "SELECT revision, content_generation FROM subscription_source WHERE id = ?1",
            [source_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
        .map(
            |(revision, generation)| -> Result<(u64, u64), StorageError> {
                Ok((
                    u64::try_from(revision).map_err(|_| StorageError::CorruptData {
                        field: "subscription.revision",
                    })?,
                    u64::try_from(generation).map_err(|_| StorageError::CorruptData {
                        field: "subscription.content_generation",
                    })?,
                ))
            },
        )
        .transpose()?;
    let Some((revision, generation)) = current else {
        return Err(StorageError::SubscriptionGenerationConflict);
    };
    if revision != expected_revision {
        return Err(StorageError::SubscriptionRevisionConflict);
    }
    if generation != expected_generation {
        return Err(StorageError::SubscriptionGenerationConflict);
    }
    expected_generation
        .checked_add(1)
        .ok_or(StorageError::SubscriptionGenerationConflict)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_refresh_state(
    transaction: &Transaction<'_>,
    source_id: &str,
    expected_generation: u64,
    generation: u64,
    content_hash: [u8; 32],
    node_count: usize,
    attempted_at_unix_ms: u64,
    next_refresh_at_unix_ms: u64,
) -> Result<(), StorageError> {
    let changed = transaction.execute(
        "UPDATE subscription_source SET
             content_generation = ?1, consecutive_failures = 0,
             next_refresh_at_unix_ms = ?2, last_attempted_at_unix_ms = ?3,
             last_succeeded_at_unix_ms = ?3, last_error_code = NULL,
             content_hash = ?4, node_count = ?5, updated_at_unix_ms = ?3
         WHERE id = ?6 AND content_generation = ?7",
        params![
            to_sqlite_u64(generation)?,
            to_sqlite_u64(next_refresh_at_unix_ms)?,
            to_sqlite_u64(attempted_at_unix_ms)?,
            content_hash.as_slice(),
            i64::try_from(node_count).map_err(|_| StorageError::SubscriptionInvalid)?,
            source_id,
            to_sqlite_u64(expected_generation)?,
        ],
    )?;
    if changed != 1 {
        return Err(StorageError::SubscriptionGenerationConflict);
    }
    Ok(())
}

pub(crate) fn audit_refresh(
    transaction: &Transaction<'_>,
    source_id: &str,
    generation: u64,
    node_count: usize,
    now: u64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO audit_event(event_id, event_type, occurred_at_unix_ms, details)
         VALUES (?1, 'subscription_refreshed', ?2, ?3)",
        params![
            format!("subscription_refreshed:{source_id}:g{generation}"),
            to_sqlite_u64(now)?,
            format!("subscription_id={source_id};generation={generation};node_count={node_count}"),
        ],
    )?;
    Ok(())
}

impl SubscriptionRepository<'_> {
    #[allow(clippy::too_many_arguments)]
    pub fn record_unchanged(
        &mut self,
        source_id: &str,
        expected_source_revision: u64,
        expected_generation: u64,
        content_hash: [u8; 32],
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        if expected_generation == 0 {
            return Err(StorageError::SubscriptionGenerationConflict);
        }
        let changed = self.connection.execute(
            "UPDATE subscription_source SET
                 consecutive_failures = 0, next_refresh_at_unix_ms = ?1,
                 last_attempted_at_unix_ms = ?2, last_succeeded_at_unix_ms = ?2,
                 last_error_code = NULL, updated_at_unix_ms = ?2
             WHERE id = ?3 AND revision = ?4 AND content_generation = ?5
               AND content_hash = ?6",
            params![
                to_sqlite_u64(next_refresh_at_unix_ms)?,
                to_sqlite_u64(attempted_at_unix_ms)?,
                source_id,
                to_sqlite_u64(expected_source_revision)?,
                to_sqlite_u64(expected_generation)?,
                content_hash.as_slice(),
            ],
        )?;
        if changed != 1 {
            return Err(classify_source_conflict(
                self.connection,
                source_id,
                expected_source_revision,
            )?);
        }
        Ok(())
    }
}
