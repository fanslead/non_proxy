use nonproxy_model::OutboundId;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    CredentialKind, CredentialReference, OutboundKind, OutboundReference, StorageError,
    migration::to_sqlite_u64,
};

pub struct OutboundRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> OutboundRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save(
        &mut self,
        outbound: &OutboundReference,
        expected_current_revision: Option<u64>,
        updated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        self.save_batch(
            &[(outbound.clone(), expected_current_revision)],
            updated_at_unix_ms,
        )
    }

    pub fn save_batch(
        &mut self,
        outbounds: &[(OutboundReference, Option<u64>)],
        updated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        if outbounds.is_empty() {
            return Err(StorageError::OutboundInvalid);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (outbound, expected_revision) in outbounds {
            validate_revision(&transaction, outbound, *expected_revision)?;
        }
        validate_default_outbound(&transaction, outbounds)?;
        for (outbound, _) in outbounds {
            save_outbound(&transaction, outbound, updated_at_unix_ms)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn get(&self, outbound_id: &OutboundId) -> Result<Option<OutboundReference>, StorageError> {
        let raw = self
            .connection
            .query_row(
                "SELECT kind, endpoint_host, endpoint_port,
                        credential_reference, credential_kind,
                        credential_label, credential_version,
                        enabled, revision
                 FROM outbound WHERE id = ?1",
                [outbound_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|raw| decode_outbound(outbound_id.clone(), raw))
            .transpose()
    }

    pub fn list(&self) -> Result<Vec<OutboundReference>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM outbound ORDER BY id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        ids.into_iter()
            .map(|id| {
                let outbound_id = OutboundId::new(id)?;
                self.get(&outbound_id)?.ok_or(StorageError::CorruptData {
                    field: "outbound.id",
                })
            })
            .collect()
    }
}

pub(crate) fn validate_default_outbound(
    transaction: &Transaction<'_>,
    outbounds: &[(OutboundReference, Option<u64>)],
) -> Result<(), StorageError> {
    let default_outbound_id = transaction
        .query_row(
            "SELECT default_outbound_id
             FROM routing_settings
             WHERE singleton_id = 1 AND default_action = 'proxy'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(default_outbound_id) = default_outbound_id else {
        return Ok(());
    };
    if outbounds.iter().any(|(outbound, _)| {
        outbound.id().as_str() == default_outbound_id
            && (!outbound.enabled() || !outbound.kind().supports_default_route())
    }) {
        return Err(StorageError::DefaultOutboundUnavailable);
    }
    Ok(())
}

pub(crate) fn save_outbound(
    transaction: &Transaction<'_>,
    outbound: &OutboundReference,
    updated_at_unix_ms: u64,
) -> Result<(), StorageError> {
    let credential = outbound.credential();
    transaction.execute(
        "INSERT INTO outbound(
                id, kind, endpoint_host, endpoint_port, credential_reference,
                credential_kind, credential_label, credential_version,
                enabled, revision, updated_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                endpoint_host = excluded.endpoint_host,
                endpoint_port = excluded.endpoint_port,
                credential_reference = excluded.credential_reference,
                credential_kind = excluded.credential_kind,
                credential_label = excluded.credential_label,
                credential_version = excluded.credential_version,
                enabled = excluded.enabled,
                revision = excluded.revision,
                updated_at_unix_ms = excluded.updated_at_unix_ms",
        params![
            outbound.id().as_str(),
            outbound.kind().as_str(),
            outbound.endpoint_host(),
            outbound.endpoint_port().map(i64::from),
            credential.map(CredentialReference::item_reference),
            credential.map(|value| value.kind().as_str()),
            credential.map(CredentialReference::display_label),
            credential
                .map(CredentialReference::version)
                .map(to_sqlite_u64)
                .transpose()?,
            i64::from(outbound.enabled()),
            to_sqlite_u64(outbound.revision())?,
            to_sqlite_u64(updated_at_unix_ms)?
        ],
    )?;
    transaction.execute(
        "INSERT INTO audit_event(
                event_id, event_type, occurred_at_unix_ms, details
             ) VALUES (?1, 'outbound_saved', ?2, ?3)",
        params![
            format!(
                "outbound_saved:{}:r{}",
                outbound.id().as_str(),
                outbound.revision()
            ),
            to_sqlite_u64(updated_at_unix_ms)?,
            format!(
                "outbound_id={};revision={}",
                outbound.id().as_str(),
                outbound.revision()
            )
        ],
    )?;
    Ok(())
}

type RawOutbound = (
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    i64,
    i64,
);

fn decode_outbound(id: OutboundId, raw: RawOutbound) -> Result<OutboundReference, StorageError> {
    let credential = match (raw.3, raw.4, raw.5, raw.6) {
        (None, None, None, None) => None,
        (Some(reference), Some(kind), Some(label), Some(version)) => {
            Some(CredentialReference::new(
                reference,
                CredentialKind::parse(&kind)?,
                label,
                u64::try_from(version).map_err(|_| StorageError::CorruptData {
                    field: "outbound.credential_version",
                })?,
            )?)
        }
        _ => {
            return Err(StorageError::CorruptData {
                field: "outbound.credential",
            });
        }
    };
    let port = raw
        .2
        .map(|value| {
            u16::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "outbound.endpoint_port",
            })
        })
        .transpose()?;
    let revision = u64::try_from(raw.8).map_err(|_| StorageError::CorruptData {
        field: "outbound.revision",
    })?;
    let mut outbound = OutboundReference::new(
        id,
        OutboundKind::parse(&raw.0)?,
        raw.1.as_deref(),
        port,
        credential,
        revision,
    )?;
    match raw.7 {
        0 => outbound = outbound.disabled(),
        1 => {}
        _ => {
            return Err(StorageError::CorruptData {
                field: "outbound.enabled",
            });
        }
    }
    Ok(outbound)
}

pub(crate) fn validate_revision(
    transaction: &Transaction<'_>,
    outbound: &OutboundReference,
    expected_current_revision: Option<u64>,
) -> Result<(), StorageError> {
    let current = transaction
        .query_row(
            "SELECT revision FROM outbound WHERE id = ?1",
            [outbound.id().as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "outbound.revision",
            })
        })
        .transpose()?;
    let valid = match (current, expected_current_revision) {
        (None, None) => outbound.revision() == 1,
        (Some(current), Some(expected)) => {
            current == expected && current.checked_add(1) == Some(outbound.revision())
        }
        _ => false,
    };
    if !valid {
        return Err(StorageError::OutboundRevisionConflict);
    }
    Ok(())
}
