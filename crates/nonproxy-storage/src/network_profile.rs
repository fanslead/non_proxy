use nonproxy_model::NetworkProfileId;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{StorageError, migration::to_sqlite_u64};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkFingerprintKind {
    WifiSsidSha256,
    DefaultGatewaySha256,
    InterfaceClass,
}

impl NetworkFingerprintKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::WifiSsidSha256 => "wifi_ssid_sha256",
            Self::DefaultGatewaySha256 => "default_gateway_sha256",
            Self::InterfaceClass => "interface_class",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "wifi_ssid_sha256" => Ok(Self::WifiSsidSha256),
            "default_gateway_sha256" => Ok(Self::DefaultGatewaySha256),
            "interface_class" => Ok(Self::InterfaceClass),
            _ => Err(StorageError::CorruptData {
                field: "network_profile.fingerprint_kind",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkFingerprint {
    kind: NetworkFingerprintKind,
    value: String,
}

impl NetworkFingerprint {
    pub fn new(
        kind: NetworkFingerprintKind,
        value: impl Into<String>,
    ) -> Result<Self, StorageError> {
        let value = value.into();
        let valid = match kind {
            NetworkFingerprintKind::WifiSsidSha256
            | NetworkFingerprintKind::DefaultGatewaySha256 => {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }
            NetworkFingerprintKind::InterfaceClass => {
                matches!(value.as_str(), "wifi" | "ethernet" | "cellular" | "other")
            }
        };
        if !valid {
            return Err(StorageError::NetworkProfileInvalid);
        }
        Ok(Self { kind, value })
    }

    #[must_use]
    pub const fn kind(&self) -> NetworkFingerprintKind {
        self.kind
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkProfileReference {
    id: NetworkProfileId,
    display_name: String,
    fingerprint: NetworkFingerprint,
    revision: u64,
}

impl NetworkProfileReference {
    pub fn new(
        id: NetworkProfileId,
        display_name: impl Into<String>,
        fingerprint: NetworkFingerprint,
        revision: u64,
    ) -> Result<Self, StorageError> {
        let display_name = display_name.into();
        if display_name.is_empty()
            || display_name.len() > 128
            || display_name.chars().any(char::is_control)
            || revision == 0
        {
            return Err(StorageError::NetworkProfileInvalid);
        }
        Ok(Self {
            id,
            display_name,
            fingerprint,
            revision,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &NetworkProfileId {
        &self.id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn fingerprint(&self) -> &NetworkFingerprint {
        &self.fingerprint
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

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
                profile.fingerprint().kind().as_str(),
                profile.fingerprint().value(),
                to_sqlite_u64(profile.revision())?,
                to_sqlite_u64(updated_at_unix_ms)?
            ],
        )?;
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
            NetworkProfileReference::new(
                profile_id.clone(),
                display_name,
                NetworkFingerprint::new(NetworkFingerprintKind::parse(&kind)?, value)?,
                u64::try_from(revision).map_err(|_| StorageError::CorruptData {
                    field: "network_profile.revision",
                })?,
            )
        })
        .transpose()
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
