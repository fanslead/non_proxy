mod support;

use nonproxy_model::OutboundId;
use nonproxy_storage::{
    CredentialKind, CredentialReference, DefaultRoute, OutboundKind, OutboundReference,
    PolicyDatabase, StorageError,
};
use support::artifact;

fn outbound(id: &str, enabled: bool) -> OutboundReference {
    let id = match OutboundId::new(id) {
        Ok(value) => value,
        Err(error) => panic!("测试出口标识创建失败: {error}"),
    };
    let value = match OutboundReference::new(
        id,
        OutboundKind::Socks5,
        Some("proxy.example.com"),
        Some(1080),
        None,
        1,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试出口创建失败: {error}"),
    };
    if enabled { value } else { value.disabled() }
}

#[test]
fn initial_route_is_direct_at_revision_one() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };

    let settings = database.routing_settings().get();
    let Ok(settings) = settings else {
        panic!("默认路由读取失败: {settings:?}");
    };

    assert_eq!(settings.route(), &DefaultRoute::Direct);
    assert_eq!(settings.revision(), 1);
}

#[test]
fn proxy_route_and_snapshot_are_staged_atomically() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let outbound = outbound("primary-proxy", true);
    if let Err(error) = database.outbounds().save(&outbound, None, 1_100) {
        panic!("测试出口保存失败: {error}");
    }
    let snapshot = artifact(1, 7);
    let Ok(snapshot) = snapshot else {
        panic!("测试快照创建失败: {snapshot:?}");
    };
    let route = DefaultRoute::Proxy(outbound.id().clone());

    let result = database
        .routing_settings()
        .set_and_stage(&route, 1, &snapshot, 1_200);
    let Ok(settings) = result else {
        panic!("默认代理和快照原子保存失败: {result:?}");
    };

    assert_eq!(settings.route(), &route);
    assert_eq!(settings.revision(), 2);
    assert!(matches!(
        database.snapshots().pending(),
        Ok(Some(record)) if record.artifact() == &snapshot
    ));
}

#[test]
fn shadowsocks_can_be_selected_as_the_complete_default_route() {
    let mut database = PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("测试数据库打开失败: {error}"));
    let id = OutboundId::new("modern-default")
        .unwrap_or_else(|error| panic!("Shadowsocks 出口标识创建失败: {error}"));
    let credential = CredentialReference::new(
        "keychain:modern-default",
        CredentialKind::Password,
        "Shadowsocks 密钥",
        1,
    )
    .unwrap_or_else(|error| panic!("Shadowsocks 凭据引用创建失败: {error}"));
    let outbound = OutboundReference::new(
        id,
        OutboundKind::Shadowsocks,
        Some("ss.example"),
        Some(8_388),
        Some(credential),
        1,
    )
    .unwrap_or_else(|error| panic!("Shadowsocks 出口创建失败: {error}"));
    database
        .outbounds()
        .save(&outbound, None, 1_100)
        .unwrap_or_else(|error| panic!("Shadowsocks 出口保存失败: {error}"));
    let snapshot =
        artifact(1, 7).unwrap_or_else(|error| panic!("Shadowsocks 默认路由快照创建失败: {error}"));
    let route = DefaultRoute::Proxy(outbound.id().clone());

    let settings = database
        .routing_settings()
        .set_and_stage(&route, 1, &snapshot, 1_200)
        .unwrap_or_else(|error| panic!("Shadowsocks 默认路由选择失败: {error}"));

    assert_eq!(settings.route(), &route);
}

#[test]
fn stale_revision_changes_neither_route_nor_snapshot() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let outbound = outbound("stale-proxy", true);
    if let Err(error) = database.outbounds().save(&outbound, None, 1_100) {
        panic!("测试出口保存失败: {error}");
    }
    let snapshot = artifact(1, 8);
    let Ok(snapshot) = snapshot else {
        panic!("测试快照创建失败: {snapshot:?}");
    };

    let result = database.routing_settings().set_and_stage(
        &DefaultRoute::Proxy(outbound.id().clone()),
        9,
        &snapshot,
        1_200,
    );

    assert!(matches!(result, Err(StorageError::RoutingRevisionConflict)));
    assert!(matches!(
        database.routing_settings().get(),
        Ok(settings)
            if settings.route() == &DefaultRoute::Direct && settings.revision() == 1
    ));
    assert!(matches!(database.snapshots().pending(), Ok(None)));
}

#[test]
fn missing_disabled_or_incompatible_default_outbound_is_rejected() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let disabled = outbound("disabled-proxy", false);
    if let Err(error) = database.outbounds().save(&disabled, None, 1_100) {
        panic!("禁用测试出口保存失败: {error}");
    }
    let incompatible_id = match OutboundId::new("tcp-only-proxy") {
        Ok(value) => value,
        Err(error) => panic!("受限出口标识创建失败: {error}"),
    };
    let incompatible = OutboundReference::new(
        incompatible_id,
        OutboundKind::HttpConnect,
        Some("proxy.example.com"),
        Some(8_080),
        None,
        1,
    );
    let Ok(incompatible) = incompatible else {
        panic!("受限出口创建失败: {incompatible:?}");
    };
    if let Err(error) = database.outbounds().save(&incompatible, None, 1_100) {
        panic!("受限出口保存失败: {error}");
    }
    let snapshot = artifact(1, 9);
    let Ok(snapshot) = snapshot else {
        panic!("测试快照创建失败: {snapshot:?}");
    };
    let missing_id = match OutboundId::new("missing-proxy") {
        Ok(value) => value,
        Err(error) => panic!("缺失出口标识创建失败: {error}"),
    };

    for route in [
        DefaultRoute::Proxy(missing_id),
        DefaultRoute::Proxy(disabled.id().clone()),
        DefaultRoute::Proxy(incompatible.id().clone()),
    ] {
        assert!(matches!(
            database
                .routing_settings()
                .set_and_stage(&route, 1, &snapshot, 1_200),
            Err(StorageError::DefaultOutboundUnavailable)
        ));
    }
    assert!(matches!(database.snapshots().pending(), Ok(None)));
}

#[test]
fn pending_snapshot_rolls_back_the_route_update() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let outbound = outbound("pending-proxy", true);
    if let Err(error) = database.outbounds().save(&outbound, None, 1_100) {
        panic!("测试出口保存失败: {error}");
    }
    let existing = artifact(1, 1);
    let Ok(existing) = existing else {
        panic!("已有测试快照创建失败: {existing:?}");
    };
    if let Err(error) = database.snapshots().stage(&existing) {
        panic!("已有测试快照暂存失败: {error}");
    }
    let next = artifact(2, 2);
    let Ok(next) = next else {
        panic!("新测试快照创建失败: {next:?}");
    };

    let result = database.routing_settings().set_and_stage(
        &DefaultRoute::Proxy(outbound.id().clone()),
        1,
        &next,
        1_200,
    );

    assert!(matches!(result, Err(StorageError::PendingSnapshotExists)));
    assert!(matches!(
        database.routing_settings().get(),
        Ok(settings)
            if settings.route() == &DefaultRoute::Direct && settings.revision() == 1
    ));
    assert!(matches!(database.snapshots().latest_version(), Ok(Some(1))));
}

#[test]
fn invalid_rollback_source_rolls_back_the_route_update() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let outbound = outbound("rollback-proxy", true);
    if let Err(error) = database.outbounds().save(&outbound, None, 1_100) {
        panic!("回滚原子性测试出口保存失败: {error}");
    }

    let result = database.routing_settings().set_and_stage_rollback(
        &DefaultRoute::Proxy(outbound.id().clone()),
        1,
        1,
        99,
        1_200,
    );

    assert!(matches!(result, Err(StorageError::SnapshotNotFound)));
    assert!(matches!(
        database.routing_settings().get(),
        Ok(settings)
            if settings.route() == &DefaultRoute::Direct && settings.revision() == 1
    ));
    assert!(matches!(database.snapshots().pending(), Ok(None)));
}

#[test]
fn rebuilt_rollback_stages_new_payload_and_tracks_the_source() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let source = artifact(1, 1);
    let Ok(source) = source else {
        panic!("回滚源快照创建失败: {source:?}");
    };
    if let Err(error) = database.snapshots().stage(&source) {
        panic!("回滚源快照暂存失败: {error}");
    }
    let ack = nonproxy_storage::ProviderAck::loaded(
        "transparent-proxy",
        1,
        *source.content_hash(),
        1_100,
    );
    let Ok(ack) = ack else {
        panic!("回滚源 ACK 创建失败: {ack:?}");
    };
    if let Err(error) = database.snapshots().record_ack(1, &ack) {
        panic!("回滚源 ACK 保存失败: {error}");
    }
    if let Err(error) = database
        .snapshots()
        .activate(1, &["transparent-proxy".to_owned()], 1_200)
    {
        panic!("回滚源快照激活失败: {error}");
    }
    let rebuilt = artifact(2, 9);
    let Ok(rebuilt) = rebuilt else {
        panic!("重建回滚快照创建失败: {rebuilt:?}");
    };

    let result = database.routing_settings().set_and_stage_rebuilt_rollback(
        &DefaultRoute::Direct,
        1,
        &rebuilt,
        1,
        1,
        1_300,
    );
    let pending = database.snapshots().pending();

    assert!(matches!(result, Ok(settings) if settings.revision() == 2));
    assert!(matches!(
        pending,
        Ok(Some(record))
            if record.artifact() == &rebuilt && record.source_snapshot_version() == Some(1)
    ));
}

#[test]
fn rebuilt_rollback_rejects_a_stale_active_snapshot_atomically() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let source = artifact(1, 1).unwrap_or_else(|error| {
        panic!("回滚源快照创建失败: {error}");
    });
    if let Err(error) = database.snapshots().stage(&source) {
        panic!("回滚源快照暂存失败: {error}");
    }
    let ack = nonproxy_storage::ProviderAck::loaded(
        "transparent-proxy",
        1,
        *source.content_hash(),
        1_100,
    )
    .unwrap_or_else(|error| panic!("回滚源 ACK 创建失败: {error}"));
    if let Err(error) = database.snapshots().record_ack(1, &ack) {
        panic!("回滚源 ACK 保存失败: {error}");
    }
    if let Err(error) = database
        .snapshots()
        .activate(1, &["transparent-proxy".to_owned()], 1_200)
    {
        panic!("回滚源快照激活失败: {error}");
    }
    let rebuilt = artifact(2, 2).unwrap_or_else(|error| {
        panic!("重建回滚快照创建失败: {error}");
    });

    let result = database.routing_settings().set_and_stage_rebuilt_rollback(
        &DefaultRoute::Direct,
        1,
        &rebuilt,
        1,
        9,
        1_300,
    );

    assert!(matches!(
        result,
        Err(StorageError::ActiveSnapshotVersionConflict)
    ));
    assert!(matches!(
        database.routing_settings().get(),
        Ok(settings) if settings.revision() == 1
    ));
    assert!(matches!(database.snapshots().pending(), Ok(None)));
}
