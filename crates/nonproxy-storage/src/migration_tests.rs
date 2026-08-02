use rusqlite::{Connection, params};

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
            rebuilds_referenced_table: false,
        },
        Migration {
            version: 2,
            name: "broken",
            sql: "CREATE TABLE broken(",
            rebuilds_referenced_table: false,
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
        rebuilds_referenced_table: false,
    }];
    let first = migrate_with(&mut connection, None, 1_000, &v1);
    let Ok(first) = first else {
        panic!("V1 数据库初始化失败: {first:?}");
    };
    assert_eq!(first.current_version(), 1);
    if let Err(error) = connection.execute(
        "INSERT INTO network_profile(
             id, display_name, fingerprint_kind, fingerprint_value,
             revision, updated_at_unix_ms
         ) VALUES (?1, ?2, ?3, ?4, 1, 1500)",
        params![
            "legacy-office",
            "旧办公室",
            "wifi_ssid_sha256",
            "a".repeat(64)
        ],
    ) {
        panic!("V1 网络配置档写入失败: {error}");
    }

    let upgraded = migrate_with(&mut connection, None, 2_000, MIGRATIONS);
    let Ok(upgraded) = upgraded else {
        panic!("V1 数据库升级失败: {upgraded:?}");
    };
    assert_eq!(upgraded.previous_version(), 1);
    assert_eq!(upgraded.current_version(), 16);
    assert_eq!(
        upgraded
            .applied()
            .iter()
            .map(AppliedMigration::version)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
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
    let network_generation: i64 = match connection.query_row(
        "SELECT value FROM control_generation
         WHERE name = 'network_profile_catalog'",
        [],
        |row| row.get(0),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后网络配置档目录代数读取失败: {error}"),
    };
    assert_eq!(network_generation, 0);
    let legacy_profile: (String, String, String, i64) = match connection.query_row(
        "SELECT display_name, fingerprint_kind, fingerprint_value, revision
         FROM network_profile WHERE id = 'legacy-office'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后旧网络配置档读取失败: {error}"),
    };
    assert_eq!(
        legacy_profile,
        (
            "旧办公室".to_owned(),
            "wifi_ssid_sha256".to_owned(),
            "a".repeat(64),
            1
        )
    );
    assert!(
        connection
            .execute(
                "INSERT INTO network_profile(
                     id, display_name, fingerprint_kind, fingerprint_value,
                     revision, updated_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, 1, 2100)",
                params![
                    "duplicate-office",
                    "重复办公室",
                    "wifi_ssid_sha256",
                    "a".repeat(64)
                ],
            )
            .is_err()
    );
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
        rebuilds_referenced_table: false,
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
    assert_eq!(upgraded.current_version(), 16);
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

#[test]
fn connection_evidence_migration_enforces_normal_and_fail_open_paths() {
    let mut connection = match Connection::open_in_memory() {
        Ok(value) => value,
        Err(error) => panic!("证据迁移测试数据库打开失败: {error}"),
    };
    if let Err(error) = migrate_with(&mut connection, None, 1_000, MIGRATIONS) {
        panic!("证据迁移执行失败: {error}");
    }

    let invalid = connection.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port,
             decision_action, reason_code, provider_id, provider_generation,
             flow_id, evidence_level
         ) VALUES (
             'invalid-path', 1, 1, 'app', 'example.com', 1, 443,
             1, 'NP_TEST', 'transparent-proxy', 1, 'flow-1', 3
         )",
        [],
    );
    assert!(invalid.is_err());

    let valid = connection.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port,
             decision_action, reason_code, provider_id, provider_generation,
             flow_id, evidence_level, interface_name
         ) VALUES (
             'valid-path', 1, 1, 'app', 'example.com', 1, 443,
             1, 'NP_TEST', 'transparent-proxy', 1, 'flow-2', 3, 'en0'
         )",
        [],
    );
    assert!(valid.is_ok());

    let valid_fail_open = connection.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port,
             decision_action, failure_mode, reason_code, provider_id,
             provider_generation, flow_id, evidence_level, interface_name,
             fail_open_direct, error_code
         ) VALUES (
             'valid-fail-open', 1, 1, 'app', 'example.com', 1, 443,
             2, 2, 'NP_TEST', 'windows-wfp', 1, 'flow-3', 3, 'ifindex:12',
             1, 'NP_WINDOWS_PROXY_FAIL_OPEN_DIRECT'
         )",
        [],
    );
    assert!(valid_fail_open.is_ok());

    let closed_fail_open = connection.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port,
             decision_action, failure_mode, reason_code, provider_id,
             provider_generation, flow_id, evidence_level, interface_name,
             fail_open_direct, error_code
         ) VALUES (
             'closed-fail-open', 1, 1, 'app', 'example.com', 1, 443,
             2, 1, 'NP_TEST', 'windows-wfp', 1, 'flow-4', 3, 'ifindex:12',
             1, 'NP_WINDOWS_PROXY_FAIL_OPEN_DIRECT'
         )",
        [],
    );
    assert!(closed_fail_open.is_err());

    let unexplained_fail_open = connection.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port,
             decision_action, failure_mode, reason_code, provider_id,
             provider_generation, flow_id, evidence_level, interface_name,
             fail_open_direct
         ) VALUES (
             'unexplained-fail-open', 1, 1, 'app', 'example.com', 1, 443,
             2, 2, 'NP_TEST', 'windows-wfp', 1, 'flow-5', 3, 'ifindex:12', 1
         )",
        [],
    );
    assert!(unexplained_fail_open.is_err());
}

#[test]
fn legacy_connection_decisions_upgrade_without_fabricating_app_identity() {
    let mut connection = match Connection::open_in_memory() {
        Ok(value) => value,
        Err(error) => panic!("应用身份迁移测试数据库打开失败: {error}"),
    };
    if let Err(error) = migrate_with(&mut connection, None, 1_000, &MIGRATIONS[..11]) {
        panic!("V11 数据库初始化失败: {error}");
    }
    if let Err(error) = connection.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port,
             decision_action, reason_code, provider_id, provider_generation,
             flow_id, evidence_level
         ) VALUES (
             'legacy-identity', 1, 1, 'com.example.legacy', 'example.com', 1, 443,
             1, 'NP_POLICY_DEFAULT', 'legacy', 0, '', 2
         )",
        [],
    ) {
        panic!("V11 连接记录写入失败: {error}");
    }

    let upgraded = migrate_with(&mut connection, None, 2_000, MIGRATIONS);
    let Ok(upgraded) = upgraded else {
        panic!("V11 连接记录升级失败: {upgraded:?}");
    };
    assert_eq!(upgraded.previous_version(), 11);
    assert_eq!(upgraded.current_version(), 16);
    let identity: (Option<String>, Option<String>, Option<String>) = match connection.query_row(
        "SELECT app_signer_id, app_parent_stable_id, app_helper_group_id
         FROM connection_decision WHERE event_id = 'legacy-identity'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ) {
        Ok(value) => value,
        Err(error) => panic!("升级后应用身份读取失败: {error}"),
    };
    assert_eq!(identity, (None, None, None));
}

#[test]
fn policy_target_migration_preserves_legacy_policy_children_and_foreign_keys() {
    let mut connection = Connection::open_in_memory()
        .unwrap_or_else(|error| panic!("策略目标迁移测试数据库打开失败: {error}"));
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap_or_else(|error| panic!("策略目标迁移外键启用失败: {error}"));
    migrate_with(&mut connection, None, 1_000, &MIGRATIONS[..14])
        .unwrap_or_else(|error| panic!("V14 策略目标迁移基线创建失败: {error}"));
    connection
        .execute(
            "INSERT INTO outbound(
                 id, kind, endpoint_host, endpoint_port, enabled, revision, updated_at_unix_ms
             ) VALUES ('legacy-proxy', 'socks5', '127.0.0.1', 1080, 1, 1, 1000)",
            [],
        )
        .unwrap_or_else(|error| panic!("旧策略出口写入失败: {error}"));
    connection
        .execute(
            "INSERT INTO policy(
                 id, display_name, source_kind, decision_action, outbound_id,
                 failure_mode, priority, enabled, origin, revision,
                 app_platform, app_stable_id, app_include_helpers,
                 updated_at_unix_ms
             ) VALUES (
                 'legacy-policy', '旧代理策略', 2, 2, 'legacy-proxy',
                 1, 100, 1, 2, 1, 1, 'com.example.legacy', 0, 1000
             )",
            [],
        )
        .unwrap_or_else(|error| panic!("旧策略写入失败: {error}"));
    connection
        .execute(
            "INSERT INTO policy_transport(policy_id, transport)
             VALUES ('legacy-policy', 1)",
            [],
        )
        .unwrap_or_else(|error| panic!("旧策略传输条件写入失败: {error}"));
    connection
        .execute(
            "INSERT INTO policy_port_range(policy_id, first_port, last_port)
             VALUES ('legacy-policy', 443, 443)",
            [],
        )
        .unwrap_or_else(|error| panic!("旧策略端口条件写入失败: {error}"));

    let report = migrate_with(&mut connection, None, 2_000, &MIGRATIONS[..15])
        .unwrap_or_else(|error| panic!("策略目标 V15 迁移失败: {error}"));
    assert_eq!(report.previous_version(), 14);
    assert_eq!(report.current_version(), 15);
    let target: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT outbound_id, outbound_group_id FROM policy WHERE id = 'legacy-policy'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or_else(|error| panic!("迁移后旧策略目标读取失败: {error}"));
    assert_eq!(target, (Some("legacy-proxy".to_owned()), None));
    let children: (i64, i64) = connection
        .query_row(
            "SELECT
                 (SELECT COUNT(*) FROM policy_transport WHERE policy_id = 'legacy-policy'),
                 (SELECT COUNT(*) FROM policy_port_range WHERE policy_id = 'legacy-policy')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or_else(|error| panic!("迁移后旧策略子项读取失败: {error}"));
    assert_eq!(children, (1, 1));
    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap_or_else(|error| panic!("迁移后外键检查失败: {error}"));
    assert_eq!(foreign_key_violations, 0);
}

#[test]
fn routing_target_migration_preserves_the_legacy_default_outbound() {
    let mut connection = Connection::open_in_memory()
        .unwrap_or_else(|error| panic!("默认路由目标迁移测试数据库打开失败: {error}"));
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap_or_else(|error| panic!("默认路由目标迁移外键启用失败: {error}"));
    migrate_with(&mut connection, None, 1_000, &MIGRATIONS[..15])
        .unwrap_or_else(|error| panic!("V15 默认路由目标迁移基线创建失败: {error}"));
    connection
        .execute(
            "INSERT INTO outbound(
                 id, kind, endpoint_host, endpoint_port, enabled, revision, updated_at_unix_ms
             ) VALUES ('legacy-default', 'socks5', '127.0.0.1', 1080, 1, 1, 1000)",
            [],
        )
        .unwrap_or_else(|error| panic!("旧默认出口写入失败: {error}"));
    connection
        .execute(
            "UPDATE routing_settings
             SET default_action = 'proxy', default_outbound_id = 'legacy-default', revision = 2
             WHERE singleton_id = 1",
            [],
        )
        .unwrap_or_else(|error| panic!("旧默认路由写入失败: {error}"));

    let report = migrate_with(&mut connection, None, 2_000, MIGRATIONS)
        .unwrap_or_else(|error| panic!("默认路由目标 V16 迁移失败: {error}"));
    assert_eq!(report.previous_version(), 15);
    assert_eq!(report.current_version(), 16);
    let target: (String, Option<String>, Option<String>, i64) = connection
        .query_row(
            "SELECT default_action, default_outbound_id,
                    default_outbound_group_id, revision
             FROM routing_settings WHERE singleton_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap_or_else(|error| panic!("迁移后默认路由目标读取失败: {error}"));
    assert_eq!(
        target,
        (
            "proxy".to_owned(),
            Some("legacy-default".to_owned()),
            None,
            2,
        )
    );
    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap_or_else(|error| panic!("默认路由迁移后外键检查失败: {error}"));
    assert_eq!(foreign_key_violations, 0);
}

#[test]
fn exit_probe_migration_enforces_route_shape_and_immutable_receipts() {
    let mut connection = match Connection::open_in_memory() {
        Ok(value) => value,
        Err(error) => panic!("出口探针迁移测试数据库打开失败: {error}"),
    };
    if let Err(error) = migrate_with(&mut connection, None, 1_000, MIGRATIONS) {
        panic!("出口探针迁移执行失败: {error}");
    }
    let probe_id = "G".repeat(43);
    let key_id = "G".repeat(22);

    let invalid_route = connection.execute(
        "INSERT INTO exit_probe_receipt(
             probe_id, route_kind, outbound_id, observed_ip, ip_family,
             observed_at_unix_ms, key_id, verified_at_unix_ms
         ) VALUES (?1, 1, 'forbidden', '8.8.8.8', 1, 1000, ?2, 1000)",
        rusqlite::params![probe_id, key_id],
    );
    assert!(invalid_route.is_err());

    let valid = connection.execute(
        "INSERT INTO exit_probe_receipt(
             probe_id, route_kind, outbound_id, observed_ip, ip_family,
             observed_at_unix_ms, key_id, verified_at_unix_ms
         ) VALUES (?1, 1, NULL, '8.8.8.8', 1, 1000, ?2, 1000)",
        rusqlite::params![probe_id, key_id],
    );
    assert!(valid.is_ok());
    assert!(
        connection
            .execute("UPDATE exit_probe_receipt SET observed_ip = '8.8.4.4'", [],)
            .is_err()
    );
}
