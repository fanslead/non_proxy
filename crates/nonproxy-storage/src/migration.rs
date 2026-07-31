use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::StorageError;

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

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: INITIAL_SCHEMA,
    },
    Migration {
        version: 2,
        name: "policy_catalog_generation",
        sql: POLICY_CATALOG_GENERATION,
    },
    Migration {
        version: 3,
        name: "provider_generation",
        sql: PROVIDER_GENERATION,
    },
    Migration {
        version: 4,
        name: "learning_sessions",
        sql: LEARNING_SESSIONS,
    },
    Migration {
        version: 5,
        name: "learning_confirmations",
        sql: LEARNING_CONFIRMATIONS,
    },
    Migration {
        version: 6,
        name: "synthetic_dns_bindings",
        sql: SYNTHETIC_DNS_BINDINGS,
    },
    Migration {
        version: 7,
        name: "routing_settings",
        sql: ROUTING_SETTINGS,
    },
    Migration {
        version: 8,
        name: "connection_decision_evidence",
        sql: CONNECTION_DECISION_EVIDENCE,
    },
    Migration {
        version: 9,
        name: "fail_open_path_evidence",
        sql: FAIL_OPEN_PATH_EVIDENCE,
    },
    Migration {
        version: 10,
        name: "exit_probe_receipts",
        sql: EXIT_PROBE_RECEIPTS,
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

    let mut newly_applied = Vec::new();
    if !pending.is_empty() {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
    }

    verify_integrity(connection)?;
    Ok(MigrationReport {
        previous_version,
        current_version: migrations.last().map_or(0, |value| value.version),
        applied: newly_applied,
        metadata_backup_path,
    })
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

fn backup_metadata(
    database_path: &Path,
    schema_version: i64,
    now_unix_ms: u64,
) -> Result<std::path::PathBuf, StorageError> {
    let metadata = fs::metadata(database_path).map_err(|source| StorageError::Io {
        operation: "读取迁移前数据库元数据",
        source,
    })?;
    let mut filename = database_path
        .file_name()
        .map_or_else(|| OsString::from("nonproxy.sqlite"), OsString::from);
    filename.push(format!(
        ".schema-v{schema_version}-at-{now_unix_ms}.metadata.bak"
    ));
    let backup_path = database_path.with_file_name(filename);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup_path)
        .map_err(|source| StorageError::Io {
            operation: "创建迁移前数据库元数据备份",
            source,
        })?;
    restrict_backup_permissions(&backup_path)?;
    writeln!(
        file,
        "schema_version={schema_version}\ndatabase_size={}\ncaptured_at_unix_ms={now_unix_ms}",
        metadata.len()
    )
    .and_then(|()| file.sync_all())
    .map_err(|source| StorageError::Io {
        operation: "写入迁移前数据库元数据备份",
        source,
    })?;
    Ok(backup_path)
}

#[cfg(unix)]
fn restrict_backup_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
        StorageError::Io {
            operation: "限制迁移元数据备份权限",
            source,
        }
    })
}

#[cfg(not(unix))]
fn restrict_backup_permissions(_path: &Path) -> Result<(), StorageError> {
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
