use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::StorageError;

pub(crate) fn backup_metadata(
    database_path: &Path,
    schema_version: i64,
    now_unix_ms: u64,
) -> Result<PathBuf, StorageError> {
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
