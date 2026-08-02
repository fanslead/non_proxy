use std::path::Path;

use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::{StorageError, migration_backup::backup_metadata};

const INITIAL_SCHEMA: &str = include_str!("../../../migrations/V0001__initial_schema.sql");
const POLICY_CATALOG_GENERATION: &str =
    include_str!("../../../migrations/V0002__policy_catalog_generation.sql");
const PROVIDER_GENERATION: &str =
    include_str!("../../../migrations/V0003__provider_generation.sql");
const LEARNING_SESSIONS: &str = include_str!("../../../migrations/V0004__learning_sessions.sql");
const LEARNING_CONFIRMATIONS: &str =
    include_str!("../../../migrations/V0005__learning_confirmations.sql");
const SYNTHETIC_DNS_BINDINGS: &str =
    include_str!("../../../migrations/V0006__synthetic_dns_bindings.sql");
const ROUTING_SETTINGS: &str = include_str!("../../../migrations/V0007__routing_settings.sql");
const CONNECTION_DECISION_EVIDENCE: &str =
    include_str!("../../../migrations/V0008__connection_decision_evidence.sql");
const FAIL_OPEN_PATH_EVIDENCE: &str =
    include_str!("../../../migrations/V0009__fail_open_path_evidence.sql");
const EXIT_PROBE_RECEIPTS: &str =
    include_str!("../../../migrations/V0010__exit_probe_receipts.sql");
const NETWORK_PROFILE_CATALOG: &str =
    include_str!("../../../migrations/V0011__network_profile_catalog.sql");
const CONNECTION_DECISION_APP_IDENTITY: &str =
    include_str!("../../../migrations/V0012__connection_decision_app_identity.sql");
const SUBSCRIPTION_SOURCES: &str =
    include_str!("../../../migrations/V0013__subscription_sources.sql");
const CREDENTIAL_CLEANUP_QUEUE: &str =
    include_str!("../../../migrations/V0014__credential_cleanup_queue.sql");
const POLICY_OUTBOUND_GROUP_TARGET: &str =
    include_str!("../../../migrations/V0015__policy_outbound_group_target.sql");

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
    rebuilds_referenced_table: bool,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: INITIAL_SCHEMA,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 2,
        name: "policy_catalog_generation",
        sql: POLICY_CATALOG_GENERATION,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 3,
        name: "provider_generation",
        sql: PROVIDER_GENERATION,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 4,
        name: "learning_sessions",
        sql: LEARNING_SESSIONS,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 5,
        name: "learning_confirmations",
        sql: LEARNING_CONFIRMATIONS,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 6,
        name: "synthetic_dns_bindings",
        sql: SYNTHETIC_DNS_BINDINGS,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 7,
        name: "routing_settings",
        sql: ROUTING_SETTINGS,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 8,
        name: "connection_decision_evidence",
        sql: CONNECTION_DECISION_EVIDENCE,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 9,
        name: "fail_open_path_evidence",
        sql: FAIL_OPEN_PATH_EVIDENCE,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 10,
        name: "exit_probe_receipts",
        sql: EXIT_PROBE_RECEIPTS,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 11,
        name: "network_profile_catalog",
        sql: NETWORK_PROFILE_CATALOG,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 12,
        name: "connection_decision_app_identity",
        sql: CONNECTION_DECISION_APP_IDENTITY,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 13,
        name: "subscription_sources",
        sql: SUBSCRIPTION_SOURCES,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 14,
        name: "credential_cleanup_queue",
        sql: CREDENTIAL_CLEANUP_QUEUE,
        rebuilds_referenced_table: false,
    },
    Migration {
        version: 15,
        name: "policy_outbound_group_target",
        sql: POLICY_OUTBOUND_GROUP_TARGET,
        rebuilds_referenced_table: true,
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedMigration {
    version: i64,
    name: &'static str,
}

impl AppliedMigration {
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationReport {
    previous_version: i64,
    current_version: i64,
    applied: Vec<AppliedMigration>,
    metadata_backup_path: Option<std::path::PathBuf>,
}

impl MigrationReport {
    #[must_use]
    pub const fn previous_version(&self) -> i64 {
        self.previous_version
    }

    #[must_use]
    pub const fn current_version(&self) -> i64 {
        self.current_version
    }

    #[must_use]
    pub fn applied(&self) -> &[AppliedMigration] {
        &self.applied
    }

    #[must_use]
    pub fn metadata_backup_path(&self) -> Option<&Path> {
        self.metadata_backup_path.as_deref()
    }
}

pub(crate) fn migrate(
    connection: &mut Connection,
    database_path: Option<&Path>,
    now_unix_ms: u64,
) -> Result<MigrationReport, StorageError> {
    migrate_with(connection, database_path, now_unix_ms, MIGRATIONS)
}

fn migrate_with(
    connection: &mut Connection,
    database_path: Option<&Path>,
    now_unix_ms: u64,
    migrations: &[Migration],
) -> Result<MigrationReport, StorageError> {
    validate_migration_sequence(migrations)?;
    let applied = read_applied(connection)?;
    validate_applied(&applied, migrations)?;
    let previous_version = applied.last().map_or(0, |value| value.0);
    let pending = migrations
        .iter()
        .filter(|migration| migration.version > previous_version)
        .collect::<Vec<_>>();
    let metadata_backup_path = if pending.is_empty() {
        None
    } else {
        database_path
            .map(|path| backup_metadata(path, previous_version, now_unix_ms))
            .transpose()?
    };

    let rebuilds_referenced_table = pending
        .iter()
        .any(|migration| migration.rebuilds_referenced_table);
    if rebuilds_referenced_table {
        enable_referenced_table_rebuild(connection)?;
    }
    let apply_result = apply_pending(connection, &pending, now_unix_ms);
    let restore_result = if rebuilds_referenced_table {
        disable_referenced_table_rebuild(connection)
    } else {
        Ok(())
    };
    let newly_applied = match (apply_result, restore_result) {
        (Err(error), _) => return Err(error),
        (Ok(_), Err(error)) => return Err(error),
        (Ok(applied), Ok(())) => applied,
    };

    verify_integrity(connection)?;
    Ok(MigrationReport {
        previous_version,
        current_version: migrations.last().map_or(0, |value| value.version),
        applied: newly_applied,
        metadata_backup_path,
    })
}

fn apply_pending(
    connection: &mut Connection,
    pending: &[&Migration],
    now_unix_ms: u64,
) -> Result<Vec<AppliedMigration>, StorageError> {
    if pending.is_empty() {
        return Ok(Vec::new());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut newly_applied = Vec::with_capacity(pending.len());
    for migration in pending {
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migration(version, name, checksum, applied_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                migration.version,
                migration.name,
                checksum(migration.sql).as_slice(),
                to_sqlite_u64(now_unix_ms)?
            ],
        )?;
        newly_applied.push(AppliedMigration {
            version: migration.version,
            name: migration.name,
        });
    }
    transaction.commit()?;
    Ok(newly_applied)
}

fn enable_referenced_table_rebuild(connection: &Connection) -> Result<(), StorageError> {
    connection.pragma_update(None, "foreign_keys", "OFF")?;
    Ok(())
}

fn disable_referenced_table_rebuild(connection: &Connection) -> Result<(), StorageError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

fn read_applied(connection: &Connection) -> Result<Vec<(i64, String, Vec<u8>)>, StorageError> {
    let has_history: bool = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_schema
             WHERE type = 'table' AND name = 'schema_migration'
         )",
        [],
        |row| row.get(0),
    )?;
    if !has_history {
        let user_table_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        if user_table_count > 0 {
            return Err(StorageError::UnmanagedDatabase);
        }
        return Ok(Vec::new());
    }

    let mut statement = connection.prepare(
        "SELECT version, name, checksum
         FROM schema_migration ORDER BY version",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn validate_applied(
    applied: &[(i64, String, Vec<u8>)],
    migrations: &[Migration],
) -> Result<(), StorageError> {
    for (index, (version, name, stored_checksum)) in applied.iter().enumerate() {
        let expected_version =
            i64::try_from(index).map_err(|_| StorageError::MigrationSequence {
                expected: i64::MAX,
                actual: *version,
            })? + 1;
        if *version != expected_version {
            return Err(StorageError::MigrationSequence {
                expected: expected_version,
                actual: *version,
            });
        }
        let Some(migration) = migrations
            .iter()
            .find(|migration| migration.version == *version)
        else {
            return Err(StorageError::UnknownMigration(*version));
        };
        if name != migration.name || stored_checksum.as_slice() != checksum(migration.sql) {
            return Err(StorageError::MigrationDiverged(*version));
        }
    }
    Ok(())
}

fn validate_migration_sequence(migrations: &[Migration]) -> Result<(), StorageError> {
    for (index, migration) in migrations.iter().enumerate() {
        let expected = i64::try_from(index).map_err(|_| StorageError::MigrationSequence {
            expected: i64::MAX,
            actual: migration.version,
        })? + 1;
        if migration.version != expected {
            return Err(StorageError::MigrationSequence {
                expected,
                actual: migration.version,
            });
        }
    }
    Ok(())
}

fn verify_integrity(connection: &Connection) -> Result<(), StorageError> {
    let result: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(StorageError::IntegrityCheck(result));
    }
    let foreign_key_violation: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
        [],
        |row| row.get(0),
    )?;
    if foreign_key_violation {
        return Err(StorageError::IntegrityCheck("foreign_key_check".to_owned()));
    }
    Ok(())
}

fn checksum(sql: &str) -> [u8; 32] {
    Sha256::digest(sql.as_bytes()).into()
}

pub(crate) fn to_sqlite_u64(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::CorruptData {
        field: "unix_ms_or_version",
    })
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
