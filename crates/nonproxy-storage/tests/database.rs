mod support;

#[cfg(unix)]
use std::fs;

use nonproxy_storage::{PolicyDatabase, StorageError};
use rusqlite::Connection;

#[test]
fn file_database_migrates_once_and_backs_up_metadata() {
    let directory = match secure_tempdir() {
        Ok(value) => value,
        Err(error) => panic!("测试临时目录创建失败: {error}"),
    };
    let database_path = directory.path().join("policy.sqlite");
    let database = PolicyDatabase::open(&database_path, 1_000);
    let Ok(database) = database else {
        panic!("文件数据库打开失败: {database:?}");
    };

    assert_eq!(database.migration_report().previous_version(), 0);
    assert_eq!(database.migration_report().current_version(), 15);
    assert_eq!(database.migration_report().applied().len(), 15);
    let Some(backup_path) = database.migration_report().metadata_backup_path() else {
        panic!("首次迁移应生成 metadata 备份");
    };
    assert!(backup_path.is_file());
    assert!(database.holds_writer_lease());
    drop(database);

    let reopened = PolicyDatabase::open(&database_path, 2_000);
    let Ok(reopened) = reopened else {
        panic!("数据库重新打开失败: {reopened:?}");
    };
    assert_eq!(reopened.migration_report().previous_version(), 15);
    assert!(reopened.migration_report().applied().is_empty());
    assert!(reopened.migration_report().metadata_backup_path().is_none());
}

#[test]
fn second_writer_is_rejected_while_first_lease_is_alive() {
    let directory = match secure_tempdir() {
        Ok(value) => value,
        Err(error) => panic!("测试临时目录创建失败: {error}"),
    };
    let database_path = directory.path().join("policy.sqlite");
    let first = PolicyDatabase::open(&database_path, 1_000);
    let Ok(_first) = first else {
        panic!("首个数据库写者打开失败: {first:?}");
    };
    let second = PolicyDatabase::open(&database_path, 2_000);

    assert!(matches!(
        second,
        Err(StorageError::WriteLeaseUnavailable { .. })
    ));
}

#[test]
fn unmanaged_database_is_not_silently_rebuilt() {
    let directory = match secure_tempdir() {
        Ok(value) => value,
        Err(error) => panic!("测试临时目录创建失败: {error}"),
    };
    let database_path = directory.path().join("foreign.sqlite");
    let connection = match Connection::open(&database_path) {
        Ok(value) => value,
        Err(error) => panic!("外部测试数据库创建失败: {error}"),
    };
    if let Err(error) = connection.execute("CREATE TABLE foreign_data(value TEXT)", []) {
        panic!("外部测试表创建失败: {error}");
    }
    drop(connection);

    assert!(matches!(
        PolicyDatabase::open(&database_path, 1_000),
        Err(StorageError::UnmanagedDatabase)
    ));
}

#[test]
fn changed_migration_checksum_stops_startup() {
    let directory = match secure_tempdir() {
        Ok(value) => value,
        Err(error) => panic!("测试临时目录创建失败: {error}"),
    };
    let database_path = directory.path().join("policy.sqlite");
    let database = PolicyDatabase::open(&database_path, 1_000);
    let Ok(database) = database else {
        panic!("初始数据库打开失败: {database:?}");
    };
    drop(database);
    let connection = match Connection::open(&database_path) {
        Ok(value) => value,
        Err(error) => panic!("篡改测试数据库打开失败: {error}"),
    };
    if let Err(error) = connection.execute(
        "UPDATE schema_migration SET checksum = zeroblob(32) WHERE version = 1",
        [],
    ) {
        panic!("迁移校验和篡改失败: {error}");
    }
    drop(connection);

    assert!(matches!(
        PolicyDatabase::open(&database_path, 2_000),
        Err(StorageError::MigrationDiverged(1))
    ));
}

#[cfg(unix)]
#[test]
fn database_and_lock_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let directory = match secure_tempdir() {
        Ok(value) => value,
        Err(error) => panic!("测试临时目录创建失败: {error}"),
    };
    let database_path = directory.path().join("policy.sqlite");
    let database = PolicyDatabase::open(&database_path, 1_000);
    let Ok(database) = database else {
        panic!("权限测试数据库打开失败: {database:?}");
    };
    let database_mode = match fs::metadata(&database_path) {
        Ok(value) => value.permissions().mode() & 0o777,
        Err(error) => panic!("数据库权限读取失败: {error}"),
    };
    let lock_mode = match fs::metadata(database_path.with_extension("sqlite.writer.lock")) {
        Ok(value) => value.permissions().mode() & 0o777,
        Err(error) => panic!("写锁权限读取失败: {error}"),
    };
    let Some(backup_path) = database.migration_report().metadata_backup_path() else {
        panic!("权限测试缺少迁移 metadata 备份");
    };
    let backup_mode = match fs::metadata(backup_path) {
        Ok(value) => value.permissions().mode() & 0o777,
        Err(error) => panic!("迁移 metadata 备份权限读取失败: {error}"),
    };

    assert_eq!(database_mode, 0o600);
    assert_eq!(lock_mode, 0o600);
    assert_eq!(backup_mode, 0o600);
}

#[cfg(unix)]
#[test]
fn insecure_parent_directory_is_rejected() {
    use std::os::unix::fs::PermissionsExt;

    let directory = match secure_tempdir() {
        Ok(value) => value,
        Err(error) => panic!("测试临时目录创建失败: {error}"),
    };
    let insecure = directory.path().join("shared");
    if let Err(error) = fs::create_dir(&insecure) {
        panic!("不安全目录创建失败: {error}");
    }
    if let Err(error) = fs::set_permissions(&insecure, fs::Permissions::from_mode(0o755)) {
        panic!("不安全目录权限设置失败: {error}");
    }

    assert!(matches!(
        PolicyDatabase::open(insecure.join("policy.sqlite"), 1_000),
        Err(StorageError::InsecureParentPermissions(_))
    ));
}

fn secure_tempdir() -> Result<tempfile::TempDir, std::io::Error> {
    let directory = tempfile::tempdir()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
    }
    Ok(directory)
}
