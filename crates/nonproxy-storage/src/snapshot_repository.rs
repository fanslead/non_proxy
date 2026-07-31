use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    ProviderAck, ProviderAckState, SnapshotArtifact, SnapshotRecord, SnapshotStatus, StorageError,
    migration::to_sqlite_u64,
    snapshot_query::{read_snapshot, read_snapshot_by_status},
    types::{validate_error_code, validate_provider_id},
};

pub struct SnapshotRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> SnapshotRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn stage(&mut self, artifact: &SnapshotArtifact) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        stage_in_transaction(&transaction, artifact)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn stage_rollback(
        &mut self,
        new_snapshot_version: u64,
        source_snapshot_version: u64,
        created_at_unix_ms: u64,
    ) -> Result<SnapshotArtifact, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let artifact = stage_rollback_in_transaction(
            &transaction,
            new_snapshot_version,
            source_snapshot_version,
            created_at_unix_ms,
        )?;
        transaction.commit()?;
        Ok(artifact)
    }

    pub fn record_ack(
        &mut self,
        snapshot_version: u64,
        ack: &ProviderAck,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let snapshot =
            read_snapshot(&transaction, snapshot_version)?.ok_or(StorageError::SnapshotNotFound)?;
        if snapshot.status() != SnapshotStatus::Pending {
            return Err(StorageError::SnapshotStateConflict);
        }
        if snapshot.artifact().content_hash() != ack.content_hash() {
            return Err(StorageError::SnapshotHashMismatch);
        }
        let existing_ack = transaction
            .query_row(
                "SELECT provider_generation, content_hash, state, error_code
                 FROM policy_snapshot_ack
                 WHERE snapshot_version = ?1 AND provider_id = ?2",
                params![to_sqlite_u64(snapshot_version)?, ack.provider_id()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((generation, hash, state, error_code)) = existing_ack {
            let generation = u64::try_from(generation).map_err(|_| StorageError::CorruptData {
                field: "policy_snapshot_ack.provider_generation",
            })?;
            if generation > ack.provider_generation() {
                return Err(StorageError::SnapshotStateConflict);
            }
            if generation == ack.provider_generation() {
                let identical = hash.as_slice() == ack.content_hash()
                    && state == ack.state().as_str()
                    && error_code.as_deref() == ack.error_code();
                if identical {
                    return Ok(());
                }
                return Err(StorageError::SnapshotStateConflict);
            }
        }
        transaction.execute(
            "INSERT INTO policy_snapshot_ack(
                snapshot_version, provider_id, provider_generation, content_hash,
                state, error_code, acknowledged_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(snapshot_version, provider_id) DO UPDATE SET
                provider_generation = excluded.provider_generation,
                content_hash = excluded.content_hash,
                state = excluded.state,
                error_code = excluded.error_code,
                acknowledged_at_unix_ms = excluded.acknowledged_at_unix_ms",
            params![
                to_sqlite_u64(snapshot_version)?,
                ack.provider_id(),
                to_sqlite_u64(ack.provider_generation())?,
                ack.content_hash().as_slice(),
                ack.state().as_str(),
                ack.error_code(),
                to_sqlite_u64(ack.acknowledged_at_unix_ms())?
            ],
        )?;
        if ack.state() == ProviderAckState::Rejected {
            transaction.execute(
                "UPDATE policy_snapshot
                 SET status = 'rejected', failure_code = ?2
                 WHERE snapshot_version = ?1 AND status = 'pending'",
                params![to_sqlite_u64(snapshot_version)?, ack.error_code()],
            )?;
        }
        insert_audit(
            &transaction,
            if ack.state() == ProviderAckState::Loaded {
                "snapshot_provider_loaded"
            } else {
                "snapshot_provider_rejected"
            },
            snapshot_version,
            ack.acknowledged_at_unix_ms(),
            Some(ack.provider_id()),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn activate(
        &mut self,
        snapshot_version: u64,
        required_provider_ids: &[String],
        activated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        validate_required_providers(required_provider_ids)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let snapshot =
            read_snapshot(&transaction, snapshot_version)?.ok_or(StorageError::SnapshotNotFound)?;
        if snapshot.status() != SnapshotStatus::Pending {
            return Err(StorageError::SnapshotStateConflict);
        }
        for provider_id in required_provider_ids {
            let loaded: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM policy_snapshot_ack
                    WHERE snapshot_version = ?1 AND provider_id = ?2
                      AND state = 'loaded' AND content_hash = ?3
                 )",
                params![
                    to_sqlite_u64(snapshot_version)?,
                    provider_id,
                    snapshot.artifact().content_hash().as_slice()
                ],
                |row| row.get(0),
            )?;
            if !loaded {
                return Err(StorageError::ProviderAcknowledgementMissing);
            }
        }
        transaction.execute(
            "UPDATE policy_snapshot
             SET status = 'superseded'
             WHERE status = 'active'",
            [],
        )?;
        let changed = transaction.execute(
            "UPDATE policy_snapshot
             SET status = 'active', activated_at_unix_ms = ?2,
                 failure_code = NULL
             WHERE snapshot_version = ?1 AND status = 'pending'",
            params![
                to_sqlite_u64(snapshot_version)?,
                to_sqlite_u64(activated_at_unix_ms)?
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::SnapshotStateConflict);
        }
        insert_audit(
            &transaction,
            "snapshot_activated",
            snapshot_version,
            activated_at_unix_ms,
            None,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reject_pending(
        &mut self,
        snapshot_version: u64,
        error_code: &str,
        rejected_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        validate_error_code(error_code)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE policy_snapshot
             SET status = 'rejected', failure_code = ?2
             WHERE snapshot_version = ?1 AND status = 'pending'",
            params![to_sqlite_u64(snapshot_version)?, error_code],
        )?;
        if changed != 1 {
            return Err(StorageError::SnapshotStateConflict);
        }
        insert_audit(
            &transaction,
            "snapshot_rejected",
            snapshot_version,
            rejected_at_unix_ms,
            Some(error_code),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn active(&self) -> Result<Option<SnapshotRecord>, StorageError> {
        read_snapshot_by_status(self.connection, SnapshotStatus::Active)
    }

    pub fn pending(&self) -> Result<Option<SnapshotRecord>, StorageError> {
        read_snapshot_by_status(self.connection, SnapshotStatus::Pending)
    }

    pub fn get(&self, snapshot_version: u64) -> Result<Option<SnapshotRecord>, StorageError> {
        read_snapshot(self.connection, snapshot_version)
    }

    pub fn latest_version(&self) -> Result<Option<u64>, StorageError> {
        let value: Option<i64> = self.connection.query_row(
            "SELECT MAX(snapshot_version) FROM policy_snapshot",
            [],
            |row| row.get(0),
        )?;
        value
            .map(|value| {
                u64::try_from(value).map_err(|_| StorageError::CorruptData {
                    field: "policy_snapshot.snapshot_version",
                })
            })
            .transpose()
    }
}

pub(crate) fn stage_in_transaction(
    transaction: &Transaction<'_>,
    artifact: &SnapshotArtifact,
) -> Result<(), StorageError> {
    ensure_can_stage(transaction, artifact.snapshot_version())?;
    insert_snapshot(transaction, artifact, None)?;
    insert_audit(
        transaction,
        "snapshot_staged",
        artifact.snapshot_version(),
        artifact.created_at_unix_ms(),
        None,
    )
}

pub(crate) fn stage_rollback_in_transaction(
    transaction: &Transaction<'_>,
    new_snapshot_version: u64,
    source_snapshot_version: u64,
    created_at_unix_ms: u64,
) -> Result<SnapshotArtifact, StorageError> {
    ensure_can_stage(transaction, new_snapshot_version)?;
    let source = read_snapshot(transaction, source_snapshot_version)?
        .ok_or(StorageError::SnapshotNotFound)?;
    if !matches!(
        source.status(),
        SnapshotStatus::Active | SnapshotStatus::Superseded
    ) {
        return Err(StorageError::SnapshotStateConflict);
    }
    let artifact = SnapshotArtifact::new(
        new_snapshot_version,
        source.artifact().schema_version(),
        created_at_unix_ms,
        *source.artifact().content_hash(),
        source.artifact().policy_count(),
        source.artifact().payload().to_vec(),
    )?;
    insert_snapshot(transaction, &artifact, Some(source_snapshot_version))?;
    insert_audit(
        transaction,
        "snapshot_rollback_staged",
        new_snapshot_version,
        created_at_unix_ms,
        Some(&format!(
            "source_snapshot_version={source_snapshot_version}"
        )),
    )?;
    Ok(artifact)
}

fn ensure_can_stage(
    transaction: &Transaction<'_>,
    snapshot_version: u64,
) -> Result<(), StorageError> {
    let maximum: Option<i64> = transaction.query_row(
        "SELECT MAX(snapshot_version) FROM policy_snapshot",
        [],
        |row| row.get(0),
    )?;
    if maximum
        .is_some_and(|value| u64::try_from(value).map_or(true, |value| snapshot_version <= value))
    {
        return Err(StorageError::SnapshotVersionNotMonotonic);
    }
    let has_pending: bool = transaction.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM policy_snapshot WHERE status = 'pending'
         )",
        [],
        |row| row.get(0),
    )?;
    if has_pending {
        return Err(StorageError::PendingSnapshotExists);
    }
    Ok(())
}

fn insert_snapshot(
    transaction: &Transaction<'_>,
    artifact: &SnapshotArtifact,
    source_snapshot_version: Option<u64>,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO policy_snapshot(
            snapshot_version, source_snapshot_version, schema_version,
            created_at_unix_ms, content_hash, policy_count, payload, status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending')",
        params![
            to_sqlite_u64(artifact.snapshot_version())?,
            source_snapshot_version.map(to_sqlite_u64).transpose()?,
            i64::from(artifact.schema_version()),
            to_sqlite_u64(artifact.created_at_unix_ms())?,
            artifact.content_hash().as_slice(),
            i64::try_from(artifact.policy_count())
                .map_err(|_| { StorageError::SnapshotPayloadInvalid })?,
            artifact.payload()
        ],
    )?;
    Ok(())
}

fn validate_required_providers(values: &[String]) -> Result<(), StorageError> {
    if values.is_empty() {
        return Err(StorageError::RequiredProvidersEmpty);
    }
    let mut unique = BTreeSet::new();
    for value in values {
        validate_provider_id(value)?;
        if !unique.insert(value) {
            return Err(StorageError::ProviderIdInvalid);
        }
    }
    Ok(())
}

fn insert_audit(
    transaction: &Transaction<'_>,
    event_type: &str,
    snapshot_version: u64,
    occurred_at_unix_ms: u64,
    details: Option<&str>,
) -> Result<(), StorageError> {
    let event_id = format!(
        "{event_type}:{snapshot_version}:{occurred_at_unix_ms}:{}",
        details.unwrap_or("none")
    );
    transaction.execute(
        "INSERT OR IGNORE INTO audit_event(
            event_id, event_type, occurred_at_unix_ms, snapshot_version, details
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            event_id,
            event_type,
            to_sqlite_u64(occurred_at_unix_ms)?,
            to_sqlite_u64(snapshot_version)?,
            details
        ],
    )?;
    Ok(())
}
