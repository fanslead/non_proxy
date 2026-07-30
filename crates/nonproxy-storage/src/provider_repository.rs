use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::StorageError;

const MAX_PROVIDER_ID_LENGTH: usize = 128;

pub struct ProviderRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> ProviderRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn next_generation(&mut self, provider_id: &str) -> Result<u64, StorageError> {
        validate_provider_id(provider_id)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value: i64 = transaction.query_row(
            "INSERT INTO provider_generation(provider_id, generation)
             VALUES (?1, 1)
             ON CONFLICT(provider_id) DO UPDATE
             SET generation = generation + 1
             RETURNING generation",
            [provider_id],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        u64::try_from(value).map_err(|_| StorageError::CorruptData {
            field: "provider_generation.generation",
        })
    }

    pub fn current_generation(&self, provider_id: &str) -> Result<Option<u64>, StorageError> {
        validate_provider_id(provider_id)?;
        let value = self
            .connection
            .query_row(
                "SELECT generation FROM provider_generation WHERE provider_id = ?1",
                [provider_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        value
            .map(|value| {
                u64::try_from(value).map_err(|_| StorageError::CorruptData {
                    field: "provider_generation.generation",
                })
            })
            .transpose()
    }
}

fn validate_provider_id(value: &str) -> Result<(), StorageError> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_ID_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(StorageError::CorruptData {
            field: "provider_generation.provider_id",
        });
    }
    Ok(())
}
