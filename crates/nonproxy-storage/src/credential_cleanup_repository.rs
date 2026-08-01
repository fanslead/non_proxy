use std::collections::HashSet;

use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use crate::{StorageError, migration::to_sqlite_u64, types::validate_error_code};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialCleanupEntry {
    reference: String,
    attempts: u32,
}

impl CredentialCleanupEntry {
    #[must_use]
    pub fn reference(&self) -> &str {
        &self.reference
    }

    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }
}

pub struct CredentialCleanupRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> CredentialCleanupRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn due(
        &self,
        now_unix_ms: u64,
        limit: u32,
    ) -> Result<Vec<CredentialCleanupEntry>, StorageError> {
        if limit == 0 || limit > 100 {
            return Err(StorageError::CredentialCleanupInvalid);
        }
        let mut statement = self.connection.prepare(
            "SELECT credential_reference, attempts FROM credential_cleanup_queue
             WHERE next_attempt_at_unix_ms <= ?1
             ORDER BY next_attempt_at_unix_ms, credential_reference LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![to_sqlite_u64(now_unix_ms)?, i64::from(limit)],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        rows.map(|row| {
            let (reference, attempts) = row?;
            Ok(CredentialCleanupEntry {
                reference,
                attempts: u32::try_from(attempts).map_err(|_| StorageError::CorruptData {
                    field: "credential_cleanup.attempts",
                })?,
            })
        })
        .collect()
    }

    pub fn enqueue(
        &mut self,
        references: impl IntoIterator<Item = String>,
        now_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        enqueue_credential_cleanup(&transaction, references, now_unix_ms)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn complete(&mut self, references: &[String]) -> Result<(), StorageError> {
        if references.is_empty() {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for reference in references {
            transaction.execute(
                "DELETE FROM credential_cleanup_queue WHERE credential_reference = ?1",
                [reference],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn record_failures(
        &mut self,
        failures: &[(String, u64)],
        error_code: &str,
        attempted_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        if failures.is_empty() {
            return Ok(());
        }
        validate_error_code(error_code)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (reference, next_attempt_at_unix_ms) in failures {
            let changed = transaction.execute(
                "UPDATE credential_cleanup_queue SET attempts = attempts + 1,
                     next_attempt_at_unix_ms = ?1, last_error_code = ?2,
                     updated_at_unix_ms = ?3
                 WHERE credential_reference = ?4 AND attempts < 4294967295",
                params![
                    to_sqlite_u64(*next_attempt_at_unix_ms)?,
                    error_code,
                    to_sqlite_u64(attempted_at_unix_ms)?,
                    reference,
                ],
            )?;
            if changed != 1 {
                return Err(StorageError::CredentialCleanupInvalid);
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn count(&self) -> Result<u64, StorageError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM credential_cleanup_queue",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        u64::try_from(count).map_err(|_| StorageError::CorruptData {
            field: "credential_cleanup.count",
        })
    }
}

pub(crate) fn enqueue_credential_cleanup(
    transaction: &Transaction<'_>,
    references: impl IntoIterator<Item = String>,
    now_unix_ms: u64,
) -> Result<(), StorageError> {
    let references = references.into_iter().collect::<HashSet<_>>();
    if references.is_empty() {
        return Ok(());
    }
    let now = to_sqlite_u64(now_unix_ms)?;
    for reference in references {
        if reference.is_empty() || reference.len() > 512 {
            return Err(StorageError::CredentialCleanupInvalid);
        }
        transaction.execute(
            "INSERT INTO credential_cleanup_queue(
                 credential_reference, attempts, next_attempt_at_unix_ms,
                 last_error_code, created_at_unix_ms, updated_at_unix_ms
             ) VALUES (?1, 0, ?2, NULL, ?2, ?2)
             ON CONFLICT(credential_reference) DO UPDATE SET
                 next_attempt_at_unix_ms = MIN(
                     next_attempt_at_unix_ms,
                     excluded.next_attempt_at_unix_ms
                 ),
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![reference, now],
        )?;
    }
    Ok(())
}
