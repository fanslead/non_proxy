use nonproxy_model::{
    NetworkFingerprint, NetworkFingerprintKind, NetworkProfileId, NetworkProfileReference,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{StorageError, migration::to_sqlite_u64};

pub struct NetworkProfileRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> NetworkProfileRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save(
        &mut self,
        profile: &NetworkProfileReference,
        expected_current_revision: Option<u64>,
        updated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_revision(&transaction, profile, expected_current_revision)?;
        validate_unique_fingerprint(&transaction, profile)?;
        transaction.execute(
            "INSERT INTO network_profile(
                id, display_name, fingerprint_kind, fingerprint_value,
                revision, updated_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                fingerprint_kind = excluded.fingerprint_kind,
                fingerprint_value = excluded.fingerprint_value,
                revision = excluded.revision,
                updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                profile.id().as_str(),
                profile.display_name(),
                fingerprint_kind_code(profile.fingerprint().kind()),
                profile.fingerprint().value(),
                to_sqlite_u64(profile.revision())?,
                to_sqlite_u64(updated_at_unix_ms)?
            ],
        )?;
        transaction.execute(
            "INSERT INTO audit_event(
                event_id, event_type, occurred_at_unix_ms, details
             ) VALUES (?1, 'network_profile_saved', ?2, ?3)",
            params![
                format!(
                    "network_profile_saved:{}:{updated_at_unix_ms}",
                    profile.id().as_str()
                ),
                to_sqlite_u64(updated_at_unix_ms)?,
                profile.id().as_str()
            ],
        )?;
        increment_catalog_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get(
        &self,
        profile_id: &NetworkProfileId,
    ) -> Result<Option<NetworkProfileReference>, StorageError> {
        let raw = self
            .connection
            .query_row(
                "SELECT display_name, fingerprint_kind, fingerprint_value,
                        revision
                 FROM network_profile WHERE id = ?1",
                [profile_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|(display_name, kind, value, revision)| {
            decode_profile(profile_id.clone(), display_name, kind, value, revision)
        })
        .transpose()
    }

    pub fn list(&self) -> Result<Vec<NetworkProfileReference>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, display_name, fingerprint_kind, fingerprint_value, revision
             FROM network_profile ORDER BY id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, display_name, kind, value, revision)| {
                decode_profile(
                    NetworkProfileId::new(id)?,
                    display_name,
                    kind,
                    value,
                    revision,
                )
            })
            .collect()
    }

    pub fn delete(
        &mut self,
        profile_id: &NetworkProfileId,
        expected_current_revision: u64,
        deleted_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = transaction
            .query_row(
                "SELECT revision FROM network_profile WHERE id = ?1",
                [profile_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if current != Some(to_sqlite_u64(expected_current_revision)?) {
            return Err(StorageError::NetworkProfileRevisionConflict);
        }
        let referenced: bool = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM policy WHERE network_profile_id = ?1
             )",
            [profile_id.as_str()],
            |row| row.get(0),
        )?;
        if referenced {
            return Err(StorageError::NetworkProfileInUse);
        }
        let changed = transaction.execute(
            "DELETE FROM network_profile WHERE id = ?1 AND revision = ?2",
            params![
                profile_id.as_str(),
                to_sqlite_u64(expected_current_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::NetworkProfileRevisionConflict);
        }
        transaction.execute(
            "INSERT INTO audit_event(
                event_id, event_type, occurred_at_unix_ms, details
             ) VALUES (?1, 'network_profile_deleted', ?2, ?3)",
            params![
                format!(
                    "network_profile_deleted:{}:{deleted_at_unix_ms}",
                    profile_id.as_str()
                ),
                to_sqlite_u64(deleted_at_unix_ms)?,
                profile_id.as_str()
            ],
        )?;
        increment_catalog_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn catalog_generation(&self) -> Result<u64, StorageError> {
        let value: i64 = self.connection.query_row(
            "SELECT value FROM control_generation
             WHERE name = 'network_profile_catalog'",
            [],
            |row| row.get(0),
        )?;
        u64::try_from(value).map_err(|_| StorageError::CorruptData {
            field: "control_generation.network_profile_catalog",
        })
    }
}

fn decode_profile(
    id: NetworkProfileId,
    display_name: String,
    kind: String,
    value: String,
    revision: i64,
) -> Result<NetworkProfileReference, StorageError> {
    Ok(NetworkProfileReference::new(
        id,
        display_name,
        NetworkFingerprint::new(parse_fingerprint_kind(&kind)?, value)?,
        u64::try_from(revision).map_err(|_| StorageError::CorruptData {
            field: "network_profile.revision",
        })?,
    )?)
}

fn validate_unique_fingerprint(
    transaction: &Transaction<'_>,
    profile: &NetworkProfileReference,
) -> Result<(), StorageError> {
    let duplicate: bool = transaction.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM network_profile
            WHERE fingerprint_kind = ?1
              AND fingerprint_value = ?2
              AND id != ?3
         )",
        params![
            fingerprint_kind_code(profile.fingerprint().kind()),
            profile.fingerprint().value(),
            profile.id().as_str()
        ],
        |row| row.get(0),
    )?;
    if duplicate {
        return Err(StorageError::NetworkProfileFingerprintConflict);
    }
    Ok(())
}

fn increment_catalog_generation(transaction: &Transaction<'_>) -> Result<(), StorageError> {
    let changed = transaction.execute(
        "UPDATE control_generation
         SET value = value + 1
         WHERE name = 'network_profile_catalog'",
        [],
    )?;
    if changed != 1 {
        return Err(StorageError::CorruptData {
            field: "control_generation.network_profile_catalog",
        });
    }
    Ok(())
}

const fn fingerprint_kind_code(kind: NetworkFingerprintKind) -> &'static str {
    match kind {
        NetworkFingerprintKind::WifiSsidSha256 => "wifi_ssid_sha256",
        NetworkFingerprintKind::DefaultGatewaySha256 => "default_gateway_sha256",
        NetworkFingerprintKind::InterfaceClass => "interface_class",
    }
}

fn parse_fingerprint_kind(value: &str) -> Result<NetworkFingerprintKind, StorageError> {
    match value {
        "wifi_ssid_sha256" => Ok(NetworkFingerprintKind::WifiSsidSha256),
        "default_gateway_sha256" => Ok(NetworkFingerprintKind::DefaultGatewaySha256),
        "interface_class" => Ok(NetworkFingerprintKind::InterfaceClass),
        _ => Err(StorageError::CorruptData {
            field: "network_profile.fingerprint_kind",
        }),
    }
}

fn validate_revision(
    transaction: &Transaction<'_>,
    profile: &NetworkProfileReference,
    expected_current_revision: Option<u64>,
) -> Result<(), StorageError> {
    let current = transaction
        .query_row(
            "SELECT revision FROM network_profile WHERE id = ?1",
            [profile.id().as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "network_profile.revision",
            })
        })
        .transpose()?;
    let valid = match (current, expected_current_revision) {
        (None, None) => profile.revision() == 1,
        (Some(current), Some(expected)) => {
            current == expected && current.checked_add(1) == Some(profile.revision())
        }
        _ => false,
    };
    if !valid {
        return Err(StorageError::NetworkProfileRevisionConflict);
    }
    Ok(())
}
