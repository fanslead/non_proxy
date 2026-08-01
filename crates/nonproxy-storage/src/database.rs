use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags};

use crate::{
    ConnectionDecisionRepository, CredentialCleanupRepository, ExitProbeRepository,
    LearningConfirmationRepository, LearningRepository, MigrationReport, NetworkProfileRepository,
    OutboundRepository, PolicyRepository, ProviderRepository, RetentionRepository,
    RoutingSettingsRepository, SnapshotRepository, StorageError, SubscriptionRepository,
    SyntheticDnsRepository, migration::migrate,
};

#[derive(Debug)]
pub struct PolicyDatabase {
    connection: Connection,
    writer_lease: Option<File>,
    path: Option<PathBuf>,
    migration_report: MigrationReport,
}

impl PolicyDatabase {
    pub fn open(path: impl AsRef<Path>, now_unix_ms: u64) -> Result<Self, StorageError> {
        let path = path.as_ref();
        validate_path(path)?;
        let writer_lease = acquire_writer_lease(path)?;
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )?;
        restrict_file_permissions(path)?;
        configure(&connection, true)?;
        let migration_report = migrate(&mut connection, Some(path), now_unix_ms)?;
        Ok(Self {
            connection,
            writer_lease: Some(writer_lease),
            path: Some(path.to_path_buf()),
            migration_report,
        })
    }

    pub fn open_in_memory(now_unix_ms: u64) -> Result<Self, StorageError> {
        let mut connection = Connection::open_in_memory()?;
        configure(&connection, false)?;
        let migration_report = migrate(&mut connection, None, now_unix_ms)?;
        Ok(Self {
            connection,
            writer_lease: None,
            path: None,
            migration_report,
        })
    }

    #[must_use]
    pub const fn migration_report(&self) -> &MigrationReport {
        &self.migration_report
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn policies(&mut self) -> PolicyRepository<'_> {
        PolicyRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn outbounds(&mut self) -> OutboundRepository<'_> {
        OutboundRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn network_profiles(&mut self) -> NetworkProfileRepository<'_> {
        NetworkProfileRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn snapshots(&mut self) -> SnapshotRepository<'_> {
        SnapshotRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn routing_settings(&mut self) -> RoutingSettingsRepository<'_> {
        RoutingSettingsRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn connection_decisions(&mut self) -> ConnectionDecisionRepository<'_> {
        ConnectionDecisionRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn exit_probes(&mut self) -> ExitProbeRepository<'_> {
        ExitProbeRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn providers(&mut self) -> ProviderRepository<'_> {
        ProviderRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn retention(&mut self) -> RetentionRepository<'_> {
        RetentionRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn learning(&mut self) -> LearningRepository<'_> {
        LearningRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn learning_confirmations(&mut self) -> LearningConfirmationRepository<'_> {
        LearningConfirmationRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn synthetic_dns(&mut self) -> SyntheticDnsRepository<'_> {
        SyntheticDnsRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn subscriptions(&mut self) -> SubscriptionRepository<'_> {
        SubscriptionRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn credential_cleanup(&mut self) -> CredentialCleanupRepository<'_> {
        CredentialCleanupRepository::new(&mut self.connection)
    }

    #[must_use]
    pub fn holds_writer_lease(&self) -> bool {
        self.writer_lease.is_some()
    }
}

fn validate_path(path: &Path) -> Result<(), StorageError> {
    let Some(parent) = path.parent() else {
        return Err(StorageError::ParentDirectoryMissing(path.to_path_buf()));
    };
    if !parent.is_dir() {
        return Err(StorageError::ParentDirectoryMissing(parent.to_path_buf()));
    }
    reject_symlink(parent)?;
    validate_parent_permissions(parent)?;
    reject_symlink(path)?;
    let lock_path = writer_lock_path(path);
    reject_symlink(&lock_path)
}

#[cfg(unix)]
fn validate_parent_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|source| StorageError::Io {
        operation: "读取数据库父目录权限",
        source,
    })?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(StorageError::InsecureParentPermissions(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_parent_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(StorageError::SymlinkPathRejected(path.to_path_buf()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(StorageError::Io {
            operation: "检查数据库路径",
            source,
        }),
    }
}

fn acquire_writer_lease(path: &Path) -> Result<File, StorageError> {
    let lock_path = writer_lock_path(path);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|source| StorageError::Io {
            operation: "打开数据库写锁文件",
            source,
        })?;
    restrict_file_permissions(&lock_path)?;
    file.try_lock()
        .map_err(|source| StorageError::WriteLeaseUnavailable {
            source: source.into(),
        })?;
    Ok(file)
}

fn writer_lock_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".writer.lock");
    PathBuf::from(value)
}

fn configure(connection: &Connection, file_backed: bool) -> Result<(), StorageError> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "trusted_schema", "OFF")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    if file_backed {
        connection.pragma_update(None, "journal_mode", "WAL")?;
    } else {
        connection.pragma_update(None, "journal_mode", "MEMORY")?;
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
        StorageError::Io {
            operation: "限制数据库文件权限",
            source,
        }
    })
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}
