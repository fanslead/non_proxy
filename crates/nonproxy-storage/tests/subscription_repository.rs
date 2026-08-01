use nonproxy_model::OutboundId;
use nonproxy_storage::{
    CredentialKind, CredentialReference, DefaultRoute, OutboundKind, OutboundReference,
    PolicyDatabase, SnapshotArtifact, StorageError, SubscriptionNode, SubscriptionSource,
};

#[test]
fn source_round_trip_preserves_only_credential_reference_and_refresh_state() {
    let mut database = database();
    let source = source("office", 1, 1_000);
    database
        .subscriptions()
        .save(&source, None, 1_000)
        .unwrap_or_else(|error| panic!("订阅源保存失败: {error}"));

    let loaded = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("订阅源没有写入"));
    assert_eq!(loaded, source);
    assert_eq!(
        loaded.endpoint_credential().kind(),
        CredentialKind::SubscriptionUrl
    );
    assert_eq!(loaded.content_generation(), 0);

    database
        .subscriptions()
        .record_failure("office", 1, 0, "NP_SUBSCRIPTION_TIMEOUT", 1_100, 2_500)
        .unwrap_or_else(|error| panic!("订阅失败状态保存失败: {error}"));
    let failed = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("失败状态读取失败: {error}"))
        .unwrap_or_else(|| panic!("失败状态订阅源不存在"));
    assert_eq!(failed.consecutive_failures(), 1);
    assert_eq!(failed.last_error_code(), Some("NP_SUBSCRIPTION_TIMEOUT"));
    assert!(
        database
            .subscriptions()
            .due(2_499, 10)
            .unwrap_or_else(|error| panic!("未到期订阅查询失败: {error}"))
            .is_empty()
    );
    assert_eq!(
        database
            .subscriptions()
            .due(2_500, 10)
            .unwrap_or_else(|error| panic!("到期订阅查询失败: {error}"))
            .len(),
        1
    );

    let updated = failed
        .reconfigured(
            "办公室订阅",
            endpoint_credential("office", 2),
            false,
            3_600,
            2,
        )
        .unwrap_or_else(|error| panic!("订阅源重配置失败: {error}"));
    database
        .subscriptions()
        .save(&updated, Some(1), 1_200)
        .unwrap_or_else(|error| panic!("订阅源更新失败: {error}"));
    assert!(
        database
            .subscriptions()
            .due(3_000, 10)
            .unwrap_or_else(|error| panic!("禁用订阅查询失败: {error}"))
            .is_empty()
    );
    assert!(matches!(
        database.subscriptions().save(&updated, Some(1), 1_300),
        Err(StorageError::SubscriptionRevisionConflict)
    ));
}

#[test]
fn refresh_atomically_updates_nodes_retires_missing_and_tracks_credential_cleanup() {
    let mut database = database_with_source("office");
    let first = vec![
        node("node-a", "subscription-office-a", 1, None, "first"),
        node("node-b", "subscription-office-b", 1, None, "first"),
    ];
    let committed = database
        .subscriptions()
        .apply_refresh("office", 1, 0, [1; 32], &first, 1_100, 4_700)
        .unwrap_or_else(|error| panic!("首次订阅刷新失败: {error}"));
    assert_eq!(committed.generation(), 1);
    assert!(committed.replaced_credential_references().is_empty());
    assert!(committed.retired_outbound_ids().is_empty());

    let second = vec![
        node("node-a", "subscription-office-a", 2, Some(1), "second"),
        node("node-c", "subscription-office-c", 1, None, "second"),
    ];
    let committed = database
        .subscriptions()
        .apply_refresh("office", 1, 1, [2; 32], &second, 2_000, 5_600)
        .unwrap_or_else(|error| panic!("第二次订阅刷新失败: {error}"));

    assert_eq!(committed.generation(), 2);
    assert_eq!(
        committed.replaced_credential_references(),
        &["outbound:subscription-office-a:v1:first".to_owned()]
    );
    assert_eq!(committed.retired_outbound_ids().len(), 1);
    assert_eq!(
        committed.retired_outbound_ids()[0].as_str(),
        "subscription-office-b"
    );
    let retired_id = OutboundId::new("subscription-office-b")
        .unwrap_or_else(|error| panic!("退役出口标识创建失败: {error}"));
    let retired = database
        .outbounds()
        .get(&retired_id)
        .unwrap_or_else(|error| panic!("退役出口读取失败: {error}"))
        .unwrap_or_else(|| panic!("退役出口不存在"));
    assert!(!retired.enabled());
    assert_eq!(retired.revision(), 2);

    let ownership = database
        .subscriptions()
        .ownership("office")
        .unwrap_or_else(|error| panic!("订阅节点归属读取失败: {error}"));
    assert_eq!(ownership.len(), 3);
    let missing = ownership
        .iter()
        .find(|value| value.node_key() == "node-b")
        .unwrap_or_else(|| panic!("退役节点归属丢失"));
    assert!(!missing.present());
    assert_eq!(missing.last_seen_generation(), 1);
    let source = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("刷新后订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("刷新后订阅源不存在"));
    assert_eq!(source.content_generation(), 2);
    assert_eq!(source.node_count(), 2);
    assert_eq!(source.content_hash(), Some([2; 32]));
    assert_eq!(source.consecutive_failures(), 0);
    let cleanup = database
        .credential_cleanup()
        .due(2_000, 10)
        .unwrap_or_else(|error| panic!("刷新凭据清理队列读取失败: {error}"));
    assert_eq!(cleanup.len(), 1);
    assert_eq!(
        cleanup[0].reference(),
        "outbound:subscription-office-a:v1:first"
    );
}

#[test]
fn deletion_removes_owned_outbounds_and_persists_idempotent_credential_cleanup() {
    let mut database = database_with_source("office");
    let nodes = vec![node("node-a", "subscription-office-a", 1, None, "first")];
    database
        .subscriptions()
        .apply_refresh("office", 1, 0, [1; 32], &nodes, 1_100, 4_700)
        .unwrap_or_else(|error| panic!("删除测试订阅刷新失败: {error}"));

    let deleted = database
        .subscriptions()
        .delete("office", 1, 1_200)
        .unwrap_or_else(|error| panic!("订阅删除失败: {error}"));
    assert_eq!(deleted.outbound_count(), 1);
    assert_eq!(deleted.credential_references().len(), 2);
    assert!(
        database
            .subscriptions()
            .get("office")
            .unwrap_or_else(|error| panic!("删除后订阅读取失败: {error}"))
            .is_none()
    );
    let outbound_id = OutboundId::new("subscription-office-a")
        .unwrap_or_else(|error| panic!("删除测试出口标识创建失败: {error}"));
    assert!(
        database
            .outbounds()
            .get(&outbound_id)
            .unwrap_or_else(|error| panic!("删除后出口读取失败: {error}"))
            .is_none()
    );

    let cleanup = database
        .credential_cleanup()
        .due(1_200, 10)
        .unwrap_or_else(|error| panic!("删除凭据清理队列读取失败: {error}"));
    assert_eq!(cleanup.len(), 2);
    let failed_reference = cleanup[0].reference().to_owned();
    database
        .credential_cleanup()
        .complete(&[cleanup[1].reference().to_owned()])
        .unwrap_or_else(|error| panic!("凭据清理完成记录失败: {error}"));
    database
        .credential_cleanup()
        .record_failures(
            &[(failed_reference.clone(), 61_200)],
            "NP_CREDENTIAL_STORE_FAILED",
            1_200,
        )
        .unwrap_or_else(|error| panic!("凭据清理失败记录失败: {error}"));
    assert!(
        database
            .credential_cleanup()
            .due(61_199, 10)
            .unwrap_or_else(|error| panic!("未到期凭据清理读取失败: {error}"))
            .is_empty()
    );
    let retry = database
        .credential_cleanup()
        .due(61_200, 10)
        .unwrap_or_else(|error| panic!("到期凭据清理读取失败: {error}"));
    assert_eq!(retry.len(), 1);
    assert_eq!(retry[0].reference(), failed_reference);
    assert_eq!(retry[0].attempts(), 1);
}

#[test]
fn deletion_rejects_an_owned_default_outbound_without_partial_cleanup() {
    let mut database = database_with_source("office");
    let nodes = vec![node("node-a", "subscription-office-a", 1, None, "first")];
    database
        .subscriptions()
        .apply_refresh("office", 1, 0, [1; 32], &nodes, 1_100, 4_700)
        .unwrap_or_else(|error| panic!("默认出口删除测试刷新失败: {error}"));
    let default_id = OutboundId::new("subscription-office-a")
        .unwrap_or_else(|error| panic!("默认出口删除测试标识创建失败: {error}"));
    let snapshot = SnapshotArtifact::new(1, 1, 1_200, [9; 32], 0, vec![1])
        .unwrap_or_else(|error| panic!("默认出口删除测试快照创建失败: {error}"));
    database
        .routing_settings()
        .set_and_stage(&DefaultRoute::Proxy(default_id), 1, &snapshot, 1_200)
        .unwrap_or_else(|error| panic!("默认出口删除测试路由保存失败: {error}"));

    assert!(matches!(
        database.subscriptions().delete("office", 1, 1_300),
        Err(StorageError::SubscriptionDefaultOutboundRemoved)
    ));
    assert!(
        database
            .subscriptions()
            .get("office")
            .unwrap_or_else(|error| panic!("默认出口拒绝后订阅读取失败: {error}"))
            .is_some()
    );
    assert_eq!(
        database
            .credential_cleanup()
            .count()
            .unwrap_or_else(|error| panic!("默认出口拒绝后清理队列计数失败: {error}")),
        0
    );
}

#[test]
fn refresh_cannot_claim_a_manual_outbound_and_rolls_back_generation() {
    let mut database = database_with_source("office");
    let manual = outbound("manual-proxy", 1, "manual");
    database
        .outbounds()
        .save(&manual, None, 1_000)
        .unwrap_or_else(|error| panic!("手工出口保存失败: {error}"));
    let nodes = vec![
        SubscriptionNode::new("node-a", manual, Some(1))
            .unwrap_or_else(|error| panic!("冲突订阅节点创建失败: {error}")),
    ];

    assert!(matches!(
        database
            .subscriptions()
            .apply_refresh("office", 1, 0, [3; 32], &nodes, 1_100, 4_700),
        Err(StorageError::SubscriptionOwnershipConflict)
    ));
    let source = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("冲突后订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("冲突后订阅源不存在"));
    assert_eq!(source.content_generation(), 0);
    assert!(
        database
            .subscriptions()
            .ownership("office")
            .unwrap_or_else(|error| panic!("冲突后节点归属读取失败: {error}"))
            .is_empty()
    );
}

#[test]
fn removing_the_active_default_node_rejects_the_whole_refresh() {
    let mut database = database_with_source("office");
    let first = vec![
        node("node-a", "subscription-office-a", 1, None, "first"),
        node("node-b", "subscription-office-b", 1, None, "first"),
    ];
    database
        .subscriptions()
        .apply_refresh("office", 1, 0, [1; 32], &first, 1_100, 4_700)
        .unwrap_or_else(|error| panic!("首次订阅刷新失败: {error}"));
    let default_id = OutboundId::new("subscription-office-a")
        .unwrap_or_else(|error| panic!("默认出口标识创建失败: {error}"));
    let snapshot = SnapshotArtifact::new(1, 1, 1_200, [9; 32], 0, vec![1])
        .unwrap_or_else(|error| panic!("默认出口测试快照创建失败: {error}"));
    database
        .routing_settings()
        .set_and_stage(
            &DefaultRoute::Proxy(default_id.clone()),
            1,
            &snapshot,
            1_200,
        )
        .unwrap_or_else(|error| panic!("默认订阅出口设置失败: {error}"));
    let second = vec![node(
        "node-b",
        "subscription-office-b",
        2,
        Some(1),
        "second",
    )];

    assert!(matches!(
        database
            .subscriptions()
            .apply_refresh("office", 1, 1, [2; 32], &second, 2_000, 5_600),
        Err(StorageError::SubscriptionDefaultOutboundRemoved)
    ));
    assert_eq!(
        database
            .subscriptions()
            .get("office")
            .unwrap_or_else(|error| panic!("默认出口冲突后订阅源读取失败: {error}"))
            .unwrap_or_else(|| panic!("默认出口冲突后订阅源不存在"))
            .content_generation(),
        1
    );
    let default = database
        .outbounds()
        .get(&default_id)
        .unwrap_or_else(|error| panic!("默认出口冲突后出口读取失败: {error}"))
        .unwrap_or_else(|| panic!("默认出口冲突后出口不存在"));
    assert!(default.enabled());
    assert_eq!(default.revision(), 1);
}

#[test]
fn stale_fetch_cannot_write_results_or_failures_after_source_reconfiguration() {
    let mut database = database_with_source("office");
    let original = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("原始订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("原始订阅源不存在"));
    let updated = original
        .reconfigured(
            "新办公室订阅",
            endpoint_credential("office", 2),
            true,
            7_200,
            2,
        )
        .unwrap_or_else(|error| panic!("订阅源重配置失败: {error}"));
    database
        .subscriptions()
        .save(&updated, Some(1), 1_050)
        .unwrap_or_else(|error| panic!("订阅源重配置保存失败: {error}"));
    let stale_nodes = vec![node("node-a", "subscription-office-a", 1, None, "stale")];

    assert!(matches!(
        database
            .subscriptions()
            .apply_refresh("office", 1, 0, [8; 32], &stale_nodes, 1_100, 4_700,),
        Err(StorageError::SubscriptionRevisionConflict)
    ));
    assert!(matches!(
        database.subscriptions().record_failure(
            "office",
            1,
            0,
            "NP_SUBSCRIPTION_TIMEOUT",
            1_100,
            2_500,
        ),
        Err(StorageError::SubscriptionRevisionConflict)
    ));
    let preserved = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("重配置后订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("重配置后订阅源不存在"));
    assert_eq!(preserved.revision(), 2);
    assert_eq!(preserved.display_name(), "新办公室订阅");
    assert_eq!(preserved.content_generation(), 0);
    assert_eq!(preserved.consecutive_failures(), 0);
    assert!(preserved.last_attempted_at_unix_ms().is_none());
    assert!(
        database
            .subscriptions()
            .ownership("office")
            .unwrap_or_else(|error| panic!("重配置后节点归属读取失败: {error}"))
            .is_empty()
    );
}

#[test]
fn initial_source_and_nodes_roll_back_together_on_ownership_conflict() {
    let mut database = database();
    let conflicting = outbound("subscription-office-a", 1, "manual");
    database
        .outbounds()
        .save(&conflicting, None, 1_000)
        .unwrap_or_else(|error| panic!("冲突手工出口保存失败: {error}"));
    let nodes = vec![
        SubscriptionNode::new("node-a", conflicting, Some(1))
            .unwrap_or_else(|error| panic!("冲突订阅节点创建失败: {error}")),
    ];

    assert!(matches!(
        database.subscriptions().save_and_apply_refresh(
            &source("office", 1, 1_000),
            None,
            0,
            [5; 32],
            &nodes,
            1_100,
            4_700,
        ),
        Err(StorageError::SubscriptionOwnershipConflict)
    ));
    assert!(
        database
            .subscriptions()
            .get("office")
            .unwrap_or_else(|error| panic!("回滚后订阅源读取失败: {error}"))
            .is_none()
    );
    let id = OutboundId::new("subscription-office-a")
        .unwrap_or_else(|error| panic!("冲突出口标识创建失败: {error}"));
    assert_eq!(
        database
            .outbounds()
            .get(&id)
            .unwrap_or_else(|error| panic!("回滚后手工出口读取失败: {error}"))
            .unwrap_or_else(|| panic!("回滚后手工出口不存在"))
            .revision(),
        1
    );
}

#[test]
fn reconfiguration_rolls_back_with_nodes_and_unchanged_refresh_avoids_revision_churn() {
    let mut database = database_with_source("office");
    let first = vec![node("node-a", "subscription-office-a", 1, None, "first")];
    database
        .subscriptions()
        .apply_refresh("office", 1, 0, [6; 32], &first, 1_100, 4_700)
        .unwrap_or_else(|error| panic!("首次订阅刷新失败: {error}"));
    database
        .subscriptions()
        .record_failure("office", 1, 1, "NP_SUBSCRIPTION_TIMEOUT", 1_200, 2_500)
        .unwrap_or_else(|error| panic!("订阅失败状态保存失败: {error}"));
    database
        .subscriptions()
        .record_unchanged("office", 1, 1, [6; 32], 1_300, 4_900)
        .unwrap_or_else(|error| panic!("未变化订阅成功状态保存失败: {error}"));
    let unchanged = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("未变化订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("未变化订阅源不存在"));
    assert_eq!(unchanged.content_generation(), 1);
    assert_eq!(unchanged.consecutive_failures(), 0);
    assert_eq!(unchanged.last_succeeded_at_unix_ms(), Some(1_300));
    let updated = unchanged
        .reconfigured(
            "新办公室订阅",
            endpoint_credential("office", 2),
            true,
            7_200,
            2,
        )
        .unwrap_or_else(|error| panic!("订阅源重配置失败: {error}"));
    let stale = vec![node(
        "node-a",
        "subscription-office-a",
        2,
        Some(9),
        "second",
    )];

    assert!(matches!(
        database.subscriptions().save_and_apply_refresh(
            &updated,
            Some(1),
            1,
            [7; 32],
            &stale,
            1_400,
            8_600,
        ),
        Err(StorageError::OutboundRevisionConflict)
    ));
    let preserved = database
        .subscriptions()
        .get("office")
        .unwrap_or_else(|error| panic!("重配置回滚后订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("重配置回滚后订阅源不存在"));
    assert_eq!(preserved.revision(), 1);
    assert_eq!(preserved.display_name(), "办公室订阅");
    assert_eq!(preserved.endpoint_credential().version(), 1);
    assert_eq!(preserved.content_generation(), 1);
}

fn database() -> PolicyDatabase {
    PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("订阅测试数据库打开失败: {error}"))
}

fn database_with_source(id: &str) -> PolicyDatabase {
    let mut database = database();
    database
        .subscriptions()
        .save(&source(id, 1, 1_000), None, 1_000)
        .unwrap_or_else(|error| panic!("订阅测试源保存失败: {error}"));
    database
}

fn source(id: &str, revision: u64, next_refresh_at: u64) -> SubscriptionSource {
    SubscriptionSource::new(
        id,
        "办公室订阅",
        endpoint_credential(id, revision),
        3_600,
        revision,
        next_refresh_at,
    )
    .unwrap_or_else(|error| panic!("订阅测试源创建失败: {error}"))
}

fn endpoint_credential(id: &str, version: u64) -> CredentialReference {
    CredentialReference::new(
        format!("subscription:{id}:url:v{version}"),
        CredentialKind::SubscriptionUrl,
        format!("{id} 订阅地址"),
        version,
    )
    .unwrap_or_else(|error| panic!("订阅地址凭据引用创建失败: {error}"))
}

fn node(
    key: &str,
    id: &str,
    revision: u64,
    expected_revision: Option<u64>,
    marker: &str,
) -> SubscriptionNode {
    SubscriptionNode::new(
        key,
        outbound_with_marker(id, revision, marker),
        expected_revision,
    )
    .unwrap_or_else(|error| panic!("订阅测试节点创建失败: {error}"))
}

fn outbound(id: &str, revision: u64, marker: &str) -> OutboundReference {
    outbound_with_marker(id, revision, marker)
}

fn outbound_with_marker(id: &str, revision: u64, marker: &str) -> OutboundReference {
    let outbound_id =
        OutboundId::new(id).unwrap_or_else(|error| panic!("订阅测试出口标识创建失败: {error}"));
    let credential = CredentialReference::new(
        format!("outbound:{id}:v{revision}:{marker}"),
        CredentialKind::Password,
        format!("{id} 密钥"),
        revision,
    )
    .unwrap_or_else(|error| panic!("订阅测试出口凭据创建失败: {error}"));
    OutboundReference::new(
        outbound_id,
        OutboundKind::Shadowsocks,
        Some("proxy.example.com"),
        Some(8_388),
        Some(credential),
        revision,
    )
    .unwrap_or_else(|error| panic!("订阅测试出口创建失败: {error}"))
}
