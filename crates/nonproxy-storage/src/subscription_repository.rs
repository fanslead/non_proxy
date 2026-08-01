use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    StorageError, SubscriptionNodeOwnership, SubscriptionSource,
    migration::to_sqlite_u64,
    subscription_codec::{RawSubscriptionSource, decode_ownership, decode_source},
    types::validate_error_code,
};

const SOURCE_COLUMNS: &str =
    "id, display_name, endpoint_credential_reference, endpoint_credential_label,
     endpoint_credential_version, enabled, refresh_interval_seconds, revision,
     content_generation, consecutive_failures, next_refresh_at_unix_ms,
     last_attempted_at_unix_ms, last_succeeded_at_unix_ms, last_error_code,
     content_hash, node_count";

pub struct SubscriptionRepository<'connection> {
    pub(crate) connection: &'connection mut Connection,
}

impl<'connection> SubscriptionRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save(
        &mut self,
        source: &SubscriptionSource,
        expected_current_revision: Option<u64>,
        updated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_source_revision(&transaction, source, expected_current_revision)?;
        save_source(&transaction, source, updated_at_unix_ms)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get(&self, source_id: &str) -> Result<Option<SubscriptionSource>, StorageError> {
        let query = format!("SELECT {SOURCE_COLUMNS} FROM subscription_source WHERE id = ?1");
        self.connection
            .query_row(&query, [source_id], RawSubscriptionSource::read)
            .optional()?
            .map(decode_source)
            .transpose()
    }

    pub fn list(&self) -> Result<Vec<SubscriptionSource>, StorageError> {
        self.list_query("ORDER BY id", params![])
    }

    pub fn due(
        &self,
        now_unix_ms: u64,
        limit: u32,
    ) -> Result<Vec<SubscriptionSource>, StorageError> {
        if limit == 0 || limit > 100 {
            return Err(StorageError::SubscriptionInvalid);
        }
        self.list_query(
            "WHERE enabled = 1 AND next_refresh_at_unix_ms <= ?1
             ORDER BY next_refresh_at_unix_ms, id LIMIT ?2",
            params![to_sqlite_u64(now_unix_ms)?, i64::from(limit)],
        )
    }

    pub fn ownership(
        &self,
        source_id: &str,
    ) -> Result<Vec<SubscriptionNodeOwnership>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT outbound_id, node_key, present, last_seen_generation
             FROM subscription_outbound
             WHERE subscription_id = ?1 ORDER BY node_key",
        )?;
        let rows = statement.query_map([source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (outbound_id, node_key, present, generation) = row?;
            decode_ownership(outbound_id, node_key, present, generation)
        })
        .collect()
    }

    pub fn record_failure(
        &mut self,
        source_id: &str,
        expected_source_revision: u64,
        expected_generation: u64,
        error_code: &str,
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        validate_error_code(error_code)?;
        let changed = self.connection.execute(
            "UPDATE subscription_source SET
                 consecutive_failures = consecutive_failures + 1,
                 next_refresh_at_unix_ms = ?1,
                 last_attempted_at_unix_ms = ?2,
                 last_error_code = ?3,
                 updated_at_unix_ms = ?2
             WHERE id = ?4 AND content_generation = ?5
               AND revision = ?6
               AND consecutive_failures < 4294967295",
            params![
                to_sqlite_u64(next_refresh_at_unix_ms)?,
                to_sqlite_u64(attempted_at_unix_ms)?,
                error_code,
                source_id,
                to_sqlite_u64(expected_generation)?,
                to_sqlite_u64(expected_source_revision)?,
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

    fn list_query<P>(
        &self,
        suffix: &str,
        parameters: P,
    ) -> Result<Vec<SubscriptionSource>, StorageError>
    where
        P: rusqlite::Params,
    {
        let query = format!("SELECT {SOURCE_COLUMNS} FROM subscription_source {suffix}");
        let mut statement = self.connection.prepare(&query)?;
        let rows = statement.query_map(parameters, RawSubscriptionSource::read)?;
        rows.map(|row| decode_source(row?)).collect()
    }
}

pub(crate) fn classify_source_conflict(
    connection: &Connection,
    source_id: &str,
    expected_source_revision: u64,
) -> Result<StorageError, StorageError> {
    let current_revision = connection
        .query_row(
            "SELECT revision FROM subscription_source WHERE id = ?1",
            [source_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "subscription.revision",
            })
        })
        .transpose()?;
    Ok(if current_revision != Some(expected_source_revision) {
        StorageError::SubscriptionRevisionConflict
    } else {
        StorageError::SubscriptionGenerationConflict
    })
}

pub(crate) fn save_source(
    transaction: &Transaction<'_>,
    source: &SubscriptionSource,
    updated_at_unix_ms: u64,
) -> Result<(), StorageError> {
    let credential = source.endpoint_credential();
    transaction.execute(
        "INSERT INTO subscription_source(
             id, display_name, endpoint_credential_reference, endpoint_credential_label,
             endpoint_credential_version, enabled, refresh_interval_seconds, revision,
             content_generation, consecutive_failures, next_refresh_at_unix_ms,
             last_attempted_at_unix_ms, last_succeeded_at_unix_ms, last_error_code,
             content_hash, node_count, updated_at_unix_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(id) DO UPDATE SET
             display_name = excluded.display_name,
             endpoint_credential_reference = excluded.endpoint_credential_reference,
             endpoint_credential_label = excluded.endpoint_credential_label,
             endpoint_credential_version = excluded.endpoint_credential_version,
             enabled = excluded.enabled,
             refresh_interval_seconds = excluded.refresh_interval_seconds,
             revision = excluded.revision,
             next_refresh_at_unix_ms = excluded.next_refresh_at_unix_ms,
             updated_at_unix_ms = excluded.updated_at_unix_ms",
        params![
            source.id(),
            source.display_name(),
            credential.item_reference(),
            credential.display_label(),
            to_sqlite_u64(credential.version())?,
            i64::from(source.enabled()),
            i64::from(source.refresh_interval_seconds()),
            to_sqlite_u64(source.revision())?,
            to_sqlite_u64(source.content_generation())?,
            i64::from(source.consecutive_failures()),
            to_sqlite_u64(source.next_refresh_at_unix_ms())?,
            source
                .last_attempted_at_unix_ms()
                .map(to_sqlite_u64)
                .transpose()?,
            source
                .last_succeeded_at_unix_ms()
                .map(to_sqlite_u64)
                .transpose()?,
            source.last_error_code(),
            source.content_hash().map(|value| value.to_vec()),
            i64::from(source.node_count()),
            to_sqlite_u64(updated_at_unix_ms)?,
        ],
    )?;
    transaction.execute(
        "INSERT INTO audit_event(event_id, event_type, occurred_at_unix_ms, details)
         VALUES (?1, 'subscription_saved', ?2, ?3)",
        params![
            format!("subscription_saved:{}:r{}", source.id(), source.revision()),
            to_sqlite_u64(updated_at_unix_ms)?,
            format!(
                "subscription_id={};revision={}",
                source.id(),
                source.revision()
            ),
        ],
    )?;
    Ok(())
}

pub(crate) fn validate_source_revision(
    transaction: &Transaction<'_>,
    source: &SubscriptionSource,
    expected: Option<u64>,
) -> Result<(), StorageError> {
    let current = transaction
        .query_row(
            "SELECT revision FROM subscription_source WHERE id = ?1",
            [source.id()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "subscription.revision",
            })
        })
        .transpose()?;
    let valid = match (current, expected) {
        (None, None) => source.revision() == 1,
        (Some(current), Some(expected)) => {
            current == expected && current.checked_add(1) == Some(source.revision())
        }
        _ => false,
    };
    if !valid {
        return Err(StorageError::SubscriptionRevisionConflict);
    }
    Ok(())
}
