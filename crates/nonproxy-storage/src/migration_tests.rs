use rusqlite::Connection;

use super::*;

#[test]
fn a_failed_migration_rolls_back_the_entire_group() {
    let mut connection = match Connection::open_in_memory() {
        Ok(value) => value,
        Err(error) => panic!("迁移原子性测试数据库打开失败: {error}"),
    };
    let migrations = [
        Migration {
            version: 1,
            name: "first",
            sql: "CREATE TABLE schema_migration (
                      version INTEGER PRIMARY KEY,
                      name TEXT NOT NULL,
                      checksum BLOB NOT NULL,
                      applied_at_unix_ms INTEGER NOT NULL
                  );
                  CREATE TABLE first_table(value TEXT);",
        },
        Migration {
            version: 2,
            name: "broken",
            sql: "CREATE TABLE broken(",
        },
    ];
    assert!(migrate_with(&mut connection, None, 1_000, &migrations).is_err());
    let first_table_exists: bool = match connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_schema
            WHERE type = 'table' AND name = 'first_table'
         )",
        [],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(error) => panic!("迁移原子性结果读取失败: {error}"),
    };

    assert!(!first_table_exists);
}

#[test]
fn an_existing_v1_database_upgrades_without_reapplying_v1() {
    let mut connection = match Connection::open_in_memory() {
        Ok(value) => value,
        Err(error) => panic!("迁移升级测试数据库打开失败: {error}"),
    };
    let v1 = [Migration {
        version: 1,
        name: "initial_schema",
        sql: INITIAL_SCHEMA,
    }];
    let first = migrate_with(&mut connection, None, 1_000, &v1);
    let Ok(first) = first else {
        panic!("V1 数据库初始化失败: {first:?}");
    };
    assert_eq!(first.current_version(), 1);

    let upgraded = migrate_with(&mut connection, None, 2_000, MIGRATIONS);
    let Ok(upgraded) = upgraded else {
        panic!("V1 数据库升级失败: {upgraded:?}");
    };
    assert_eq!(upgraded.previous_version(), 1);
    assert_eq!(upgraded.current_version(), 7);
    assert_eq!(
        upgraded
            .applied()
            .iter()
            .map(AppliedMigration::version)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5, 6, 7]
    );
    let generation: i64 = match connection.query_row(
        "SELECT value FROM control_generation WHERE name = 'policy_catalog'",
        [],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后目录代数读取失败: {error}"),
    };
    assert_eq!(generation, 0);
    let routing_settings: (String, Option<String>, i64) = match connection.query_row(
        "SELECT default_action, default_outbound_id, revision
         FROM routing_settings WHERE singleton_id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后默认路由配置读取失败: {error}"),
    };
    assert_eq!(routing_settings, ("direct".to_owned(), None, 1));
}

#[test]
fn legacy_learning_rows_upgrade_without_losing_candidates() {
    let mut connection = match Connection::open_in_memory() {
        Ok(value) => value,
        Err(error) => panic!("学习迁移测试数据库打开失败: {error}"),
    };
    let v1 = [Migration {
        version: 1,
        name: "initial_schema",
        sql: INITIAL_SCHEMA,
    }];
    if let Err(error) = migrate_with(&mut connection, None, 1_000, &v1) {
        panic!("学习迁移 V1 初始化失败: {error}");
    }
    if let Err(error) = connection.execute(
        "INSERT INTO learning_session(
             id, kind, target, state, started_at_unix_ms, stopped_at_unix_ms
         ) VALUES ('legacy-session', 'site', 'example.com', 'active', 1000, NULL)",
        [],
    ) {
        panic!("旧学习会话写入失败: {error}");
    }
    if let Err(error) = connection.execute(
        "INSERT INTO learning_candidate(
             session_id, candidate_key, classification, confidence_millis,
             evidence_count, last_seen_at_unix_ms
         ) VALUES (
             'legacy-session', 'api.example.com', 'likely_api', 750, 2, 2000
         )",
        [],
    ) {
        panic!("旧学习候选写入失败: {error}");
    }

    let upgraded = migrate_with(&mut connection, None, 2_000, MIGRATIONS);
    let Ok(upgraded) = upgraded else {
        panic!("旧学习数据升级失败: {upgraded:?}");
    };
    assert_eq!(upgraded.current_version(), 7);
    let session: (String, String, i64) = match connection.query_row(
        "SELECT browser_context_id, state, expires_at_unix_ms
         FROM learning_session WHERE id = 'legacy-session'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后学习会话读取失败: {error}"),
    };
    assert_eq!(
        session,
        (
            "legacy:legacy-session".to_owned(),
            "active".to_owned(),
            61_000
        )
    );
    let candidate: (i64, i64, i64) = match connection.query_row(
        "SELECT requires_confirmation, evidence_count, subresource_count
         FROM learning_candidate
         WHERE session_id = 'legacy-session' AND candidate_key = 'api.example.com'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后学习候选读取失败: {error}"),
    };
    assert_eq!(candidate, (1, 2, 2));
}
