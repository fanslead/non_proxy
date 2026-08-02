use nonproxy_gatewayd::{Gateway, GatewayError, RuntimePolicyState, decode_snapshot_payload};
use nonproxy_learning::{
    BrowserContextId, ConfirmationId, LearningObservation, LearningObservationKind,
    LearningResourceType, LearningSession, LearningSessionId, LearningSubject, ObservationId,
};
use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, DomainName, FailureMode, OutboundGroupId,
    OutboundId, Policy, PolicyId, PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind,
    RouteAction, RuntimeOverrideMode,
};
use nonproxy_policy_compiler::{CompileCapabilities, CompileError};
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{
    DefaultRoute, LearningPolicySelection, OutboundGroup, OutboundGroupStrategy, OutboundKind,
    OutboundReference, PolicyDatabase, ProviderAck, StorageError,
};

#[tokio::test]
async fn policy_draft_round_trip_preserves_authoritative_model() {
    let gateway = gateway();
    let policy = site_policy("policy-a", "example.com", 1);

    let saved = gateway.save_policy(policy.clone(), None).await;
    let Ok(saved) = saved else {
        panic!("策略草稿保存失败: {saved:?}");
    };
    let listed = gateway.list_policies().await;
    let Ok(listed) = listed else {
        panic!("策略草稿读取失败: {listed:?}");
    };

    assert_eq!(saved, policy);
    assert_eq!(listed, vec![policy]);
}

#[tokio::test]
async fn stale_policy_revision_is_rejected_without_overwrite() {
    let gateway = gateway();
    let initial = site_policy("policy-a", "example.com", 1);
    if let Err(error) = gateway.save_policy(initial.clone(), None).await {
        panic!("初始策略保存失败: {error}");
    }
    let stale = site_policy("policy-a", "example.org", 2);

    let result = gateway.save_policy(stale, Some(9)).await;

    assert!(matches!(
        result,
        Err(GatewayError::Storage(StorageError::PolicyRevisionConflict))
    ));
    let listed = gateway.list_policies().await;
    assert!(matches!(listed, Ok(values) if values == vec![initial]));
}

#[tokio::test]
async fn validated_snapshot_is_staged_and_payload_can_be_rebuilt() {
    let gateway = gateway();
    let policy = site_policy("policy-a", "example.com", 1);
    if let Err(error) = gateway.save_policy(policy.clone(), None).await {
        panic!("策略保存失败: {error}");
    }

    let published = gateway.compile_and_stage().await;
    let Ok(published) = published else {
        panic!("策略快照发布失败: {published:?}");
    };
    let decoded = decode_snapshot_payload(published.artifact().payload());
    let Ok((policies, capabilities, default_decision)) = decoded else {
        panic!("策略快照解码失败: {decoded:?}");
    };
    let status = gateway.status().await;
    let Ok(status) = status else {
        panic!("网关状态读取失败: {status:?}");
    };

    assert_eq!(policies.len(), 2);
    assert!(policies.contains(&policy));
    assert!(policies.iter().any(|value| {
        value.id().as_str() == "system-macos-gateway-direct"
            && value.source_kind() == PolicySourceKind::System
            && value.origin() == PolicyOrigin::System
            && value.matcher().app().is_some_and(|app| {
                app.stable_id() == "com.nonproxy.gatewayd"
                    && app.platform() == nonproxy_model::Platform::MacOs
            })
            && value.decision() == &DecisionSpec::direct()
    }));
    assert_eq!(capabilities, CompileCapabilities::full());
    assert_eq!(default_decision, DecisionSpec::direct());
    assert!(status.active.is_none());
    assert_eq!(
        status
            .pending
            .as_ref()
            .map(|value| value.artifact().snapshot_version()),
        Some(1)
    );
}

#[tokio::test]
async fn outbound_group_policy_is_staged_with_authoritative_catalog() {
    let gateway = gateway();
    let primary = proxy_outbound("primary");
    let backup = proxy_outbound("backup");
    gateway
        .save_outbounds(vec![(primary.clone(), None), (backup.clone(), None)])
        .await
        .unwrap_or_else(|error| panic!("出口组成员保存失败: {error}"));
    let group_id =
        OutboundGroupId::new("automatic").unwrap_or_else(|error| panic!("出口组标识无效: {error}"));
    let group = OutboundGroup::new(
        group_id.clone(),
        "自动切换",
        OutboundGroupStrategy::Failover,
        vec![primary.id().clone(), backup.id().clone()],
        1,
    )
    .unwrap_or_else(|error| panic!("出口组无效: {error}"));
    gateway
        .save_outbound_group(group, None)
        .await
        .unwrap_or_else(|error| panic!("出口组保存失败: {error}"));
    let decision = DecisionSpec::proxy_group(group_id.clone(), FailureMode::Closed)
        .unwrap_or_else(|error| panic!("出口组决策无效: {error}"));
    let policy = site_policy_with_decision("group-policy", "example.com", 1, decision);
    gateway
        .save_policy(policy.clone(), None)
        .await
        .unwrap_or_else(|error| panic!("出口组策略保存失败: {error}"));

    let published = gateway
        .compile_and_stage()
        .await
        .unwrap_or_else(|error| panic!("出口组策略快照发布失败: {error}"));
    let (policies, capabilities, _) = decode_snapshot_payload(published.artifact().payload())
        .unwrap_or_else(|error| panic!("出口组策略快照解码失败: {error}"));
    let group = capabilities
        .outbound_groups()
        .get(&group_id)
        .unwrap_or_else(|| panic!("待发布快照缺少出口组目录"));

    assert!(policies.contains(&policy));
    assert_eq!(group.revision(), 1);
    assert_eq!(
        group.members(),
        &[primary.id().clone(), backup.id().clone()]
    );
}

#[tokio::test]
async fn selecting_default_proxy_stages_a_proxy_default_snapshot() {
    let gateway = gateway();
    let outbound = proxy_outbound("primary-proxy");
    if let Err(error) = gateway.save_outbounds(vec![(outbound.clone(), None)]).await {
        panic!("默认代理测试出口保存失败: {error}");
    }
    mark_outbound_ready(&gateway, &outbound);

    let update = gateway
        .set_default_route_and_stage(DefaultRoute::Proxy(outbound.id().clone()), 1)
        .await;
    let Ok(update) = update else {
        panic!("默认代理设置失败: {update:?}");
    };
    let decoded = decode_snapshot_payload(update.snapshot().artifact().payload());
    let Ok((_policies, _capabilities, decision)) = decoded else {
        panic!("默认代理快照解码失败: {decoded:?}");
    };
    let settings = gateway.routing_settings().await;
    let Ok(settings) = settings else {
        panic!("默认代理配置读取失败: {settings:?}");
    };

    assert_eq!(update.settings().revision(), 2);
    assert_eq!(
        update.settings().route(),
        &DefaultRoute::Proxy(outbound.id().clone())
    );
    assert_eq!(decision.action(), RouteAction::Proxy);
    assert_eq!(decision.outbound_id(), Some(outbound.id()));
    assert_eq!(settings, update.settings().clone());
}

#[tokio::test]
async fn selecting_default_group_requires_stable_health_and_stages_the_group_target() {
    let gateway = gateway();
    let primary = proxy_outbound("group-default-primary");
    let backup = proxy_outbound("group-default-backup");
    gateway
        .save_outbounds(vec![(primary.clone(), None), (backup.clone(), None)])
        .await
        .unwrap_or_else(|error| panic!("默认组成员保存失败: {error}"));
    let group_id = OutboundGroupId::new("default-failover")
        .unwrap_or_else(|error| panic!("默认组标识无效: {error}"));
    let group = OutboundGroup::new(
        group_id.clone(),
        "默认自动切换",
        OutboundGroupStrategy::Failover,
        vec![primary.id().clone(), backup.id().clone()],
        1,
    )
    .unwrap_or_else(|error| panic!("默认组配置无效: {error}"));
    gateway
        .save_outbound_group(group, None)
        .await
        .unwrap_or_else(|error| panic!("默认组保存失败: {error}"));

    mark_outbound_ready(&gateway, &primary);
    let unverified = gateway
        .set_default_route_and_stage(DefaultRoute::Group(group_id.clone()), 1)
        .await;
    assert!(matches!(
        unverified,
        Err(GatewayError::DefaultOutboundUnverified)
    ));
    assert!(
        gateway
            .status()
            .await
            .is_ok_and(|status| status.pending.is_none())
    );

    mark_outbound_ready(&gateway, &primary);
    let update = gateway
        .set_default_route_and_stage(DefaultRoute::Group(group_id.clone()), 1)
        .await
        .unwrap_or_else(|error| panic!("默认组设置失败: {error}"));
    let (_, capabilities, decision) =
        decode_snapshot_payload(update.snapshot().artifact().payload())
            .unwrap_or_else(|error| panic!("默认组快照解码失败: {error}"));

    assert_eq!(update.settings().revision(), 2);
    assert_eq!(
        update.settings().route(),
        &DefaultRoute::Group(group_id.clone())
    );
    assert_eq!(decision.action(), RouteAction::Proxy);
    assert_eq!(decision.outbound_group_id(), Some(&group_id));
    assert!(decision.outbound_id().is_none());
    assert!(capabilities.outbound_groups().contains_key(&group_id));

    let reordered = OutboundGroup::new(
        group_id.clone(),
        "默认自动切换",
        OutboundGroupStrategy::Failover,
        vec![backup.id().clone(), primary.id().clone()],
        2,
    )
    .unwrap_or_else(|error| panic!("默认组更新配置无效: {error}"));
    let blocked = gateway
        .save_outbound_group(reordered.clone(), Some(1))
        .await;
    assert!(matches!(
        blocked,
        Err(GatewayError::Storage(StorageError::PendingSnapshotExists))
    ));
    assert!(matches!(
        gateway.list_outbound_groups().await.as_deref(),
        Ok([stored]) if stored.revision() == 1
            && stored.members() == [primary.id().clone(), backup.id().clone()]
    ));

    activate(&gateway, update.snapshot()).await;
    let saved = gateway
        .save_outbound_group(reordered, Some(1))
        .await
        .unwrap_or_else(|error| panic!("默认组原子更新失败: {error}"));
    let staged = saved
        .snapshot()
        .unwrap_or_else(|| panic!("默认组更新必须生成待确认快照"));
    let (_, updated_capabilities, updated_decision) =
        decode_snapshot_payload(staged.artifact().payload())
            .unwrap_or_else(|error| panic!("默认组更新快照解码失败: {error}"));
    let updated_group = updated_capabilities
        .outbound_groups()
        .get(&group_id)
        .unwrap_or_else(|| panic!("默认组更新快照缺少组目录"));
    assert_eq!(staged.artifact().snapshot_version(), 2);
    assert_eq!(updated_group.revision(), 2);
    assert_eq!(
        updated_group.members(),
        &[backup.id().clone(), primary.id().clone()]
    );
    assert_eq!(updated_decision.outbound_group_id(), Some(&group_id));
}

#[tokio::test]
async fn runtime_override_is_time_bounded_snapshot_state_and_does_not_mutate_routing_settings() {
    let gateway = gateway();
    let too_short = gateway
        .stage_runtime_override(RuntimeOverrideMode::Direct, None, 999, 1)
        .await;
    assert!(matches!(
        too_short,
        Err(GatewayError::RuntimeOverrideDurationInvalid)
    ));
    let initial = gateway.compile_and_stage().await;
    let Ok(initial) = initial else {
        panic!("初始快照暂存失败: {initial:?}");
    };
    activate(&gateway, &initial).await;
    let before = gateway.routing_settings().await;
    let Ok(before) = before else {
        panic!("初始路由配置读取失败: {before:?}");
    };

    let stale = gateway
        .stage_runtime_override(RuntimeOverrideMode::Direct, None, 300_000, 99)
        .await;
    assert!(matches!(
        stale,
        Err(GatewayError::Storage(
            StorageError::ActiveSnapshotVersionConflict
        ))
    ));
    let after_stale = gateway.runtime_override_status().await;
    assert!(matches!(after_stale, Ok(status) if status.pending_snapshot_version.is_none()));

    let paused = gateway
        .stage_runtime_override(RuntimeOverrideMode::Paused, None, 300_000, 1)
        .await;
    let Ok(paused) = paused else {
        panic!("暂停覆盖暂存失败: {paused:?}");
    };
    let pending = gateway.runtime_override_status().await;
    let Ok(pending) = pending else {
        panic!("暂停覆盖状态读取失败: {pending:?}");
    };
    assert!(pending.active.is_none());
    assert_eq!(
        pending.pending.as_ref().map(|value| value.mode()),
        Some(RuntimeOverrideMode::Paused)
    );
    assert_eq!(pending.active_snapshot_version, Some(1));
    assert_eq!(pending.pending_snapshot_version, Some(2));
    assert!(!pending.pending_clears_override);
    assert_eq!(gateway.routing_settings().await.ok(), Some(before.clone()));

    activate(&gateway, &paused).await;
    let active = gateway.runtime_override_status().await;
    let Ok(active) = active else {
        panic!("活动覆盖状态读取失败: {active:?}");
    };
    assert_eq!(
        active.active.as_ref().map(|value| value.mode()),
        Some(RuntimeOverrideMode::Paused)
    );
    assert_eq!(active.active_snapshot_version, Some(2));

    let clearing = gateway.clear_runtime_override(2).await;
    let Ok(clearing) = clearing else {
        panic!("取消覆盖暂存失败: {clearing:?}");
    };
    let clearing_status = gateway.runtime_override_status().await;
    let Ok(clearing_status) = clearing_status else {
        panic!("取消覆盖状态读取失败: {clearing_status:?}");
    };
    assert_eq!(clearing.artifact().snapshot_version(), 3);
    assert!(clearing_status.active.is_some());
    assert!(clearing_status.pending.is_none());
    assert!(clearing_status.pending_clears_override);
    assert_eq!(gateway.routing_settings().await.ok(), Some(before));
}

#[tokio::test]
async fn ordinary_snapshot_publication_carries_the_current_unexpired_override() {
    let gateway = gateway();
    let initial = gateway
        .compile_and_stage()
        .await
        .unwrap_or_else(|error| panic!("初始快照暂存失败: {error}"));
    activate(&gateway, &initial).await;
    let override_snapshot = gateway
        .stage_runtime_override(RuntimeOverrideMode::Direct, None, 300_000, 1)
        .await
        .unwrap_or_else(|error| panic!("直连覆盖暂存失败: {error}"));
    activate(&gateway, &override_snapshot).await;

    let ordinary = gateway
        .compile_and_stage()
        .await
        .unwrap_or_else(|error| panic!("普通快照暂存失败: {error}"));
    let status = gateway
        .runtime_override_status()
        .await
        .unwrap_or_else(|error| panic!("运行态覆盖状态读取失败: {error}"));

    assert_eq!(ordinary.artifact().snapshot_version(), 3);
    assert_eq!(
        status.pending.as_ref().map(|value| value.mode()),
        Some(RuntimeOverrideMode::Direct)
    );
    assert_eq!(status.pending_snapshot_version, Some(3));
}

#[tokio::test]
async fn rollback_restores_the_source_snapshot_default_route() {
    let gateway = gateway();
    let outbound = proxy_outbound("rollback-proxy");
    if let Err(error) = gateway.save_outbounds(vec![(outbound.clone(), None)]).await {
        panic!("回滚测试出口保存失败: {error}");
    }
    mark_outbound_ready(&gateway, &outbound);
    let proxy = gateway
        .set_default_route_and_stage(DefaultRoute::Proxy(outbound.id().clone()), 1)
        .await;
    let Ok(proxy) = proxy else {
        panic!("回滚测试代理快照暂存失败: {proxy:?}");
    };
    activate(&gateway, proxy.snapshot()).await;

    let direct = gateway
        .set_default_route_and_stage(DefaultRoute::Direct, 2)
        .await;
    let Ok(direct) = direct else {
        panic!("回滚测试直连快照暂存失败: {direct:?}");
    };
    activate(&gateway, direct.snapshot()).await;

    let catalog = gateway.runtime_policy_catalog().await;
    let Ok(catalog) = catalog else {
        panic!("回滚点目录读取失败: {catalog:?}");
    };
    assert_eq!(catalog.previous_effective_snapshot_version(), Some(1));

    let stale = gateway.stage_rollback(1, 1).await;
    assert!(matches!(
        stale,
        Err(GatewayError::Storage(
            StorageError::ActiveSnapshotVersionConflict
        ))
    ));

    let rollback = gateway.stage_rollback(1, 2).await;
    let Ok(rollback) = rollback else {
        panic!("回滚快照暂存失败: {rollback:?}");
    };
    let settings = gateway.routing_settings().await;
    let Ok(settings) = settings else {
        panic!("回滚后的默认路由读取失败: {settings:?}");
    };
    let status = gateway.status().await;
    let Ok(status) = status else {
        panic!("回滚后的快照状态读取失败: {status:?}");
    };

    assert_eq!(rollback.artifact().snapshot_version(), 3);
    assert!(matches!(
        status.active,
        Some(record) if record.artifact().snapshot_version() == 2
    ));
    assert!(matches!(
        status.pending,
        Some(record) if record.artifact().snapshot_version() == 3
    ));
    assert_eq!(rollback.default_decision().action(), RouteAction::Proxy);
    assert_eq!(
        rollback.default_decision().outbound_id(),
        Some(outbound.id())
    );
    assert_eq!(
        settings.route(),
        &DefaultRoute::Proxy(outbound.id().clone())
    );
    assert_eq!(settings.revision(), 4);
}

#[tokio::test]
async fn compiler_conflict_does_not_create_a_pending_snapshot() {
    let gateway = gateway();
    let first = site_policy("policy-a", "example.com", 1);
    let second = site_policy("policy-b", "example.com", 1);
    if let Err(error) = gateway.save_policy(first, None).await {
        panic!("第一条策略保存失败: {error}");
    }
    if let Err(error) = gateway.save_policy(second, None).await {
        panic!("第二条策略保存失败: {error}");
    }

    let published = gateway.compile_and_stage().await;

    assert!(matches!(
        published,
        Err(GatewayError::Compile(CompileError::Validation { .. }))
    ));
    let status = gateway.status().await;
    assert!(matches!(status, Ok(value) if value.pending.is_none()));
}

#[tokio::test]
async fn second_pending_snapshot_is_not_silently_replaced() {
    let gateway = gateway();
    if let Err(error) = gateway
        .save_policy(site_policy("policy-a", "example.com", 1), None)
        .await
    {
        panic!("策略保存失败: {error}");
    }
    if let Err(error) = gateway.compile_and_stage().await {
        panic!("首个快照发布失败: {error}");
    }

    let second = gateway.compile_and_stage().await;

    assert!(matches!(
        second,
        Err(GatewayError::Storage(StorageError::PendingSnapshotExists))
    ));
}

#[tokio::test]
async fn runtime_status_distinguishes_draft_pending_and_newer_deletion() {
    let gateway = gateway();
    let policy = site_policy("policy-a", "example.com", 1);
    if let Err(error) = gateway.save_policy(policy, None).await {
        panic!("策略草稿保存失败: {error}");
    }
    let draft = gateway.list_runtime_policies().await;
    assert!(matches!(
        draft,
        Ok(records) if records.len() == 1
            && records[0].state() == RuntimePolicyState::Draft
    ));

    if let Err(error) = gateway.compile_and_stage().await {
        panic!("策略快照暂存失败: {error}");
    }
    let pending_catalog = gateway.runtime_policy_catalog().await;
    assert!(matches!(
        pending_catalog,
        Ok(catalog) if catalog.records().len() == 1
            && catalog.active_snapshot_version().is_none()
            && catalog.pending_snapshot_version() == Some(1)
            && catalog.records()[0].state() == RuntimePolicyState::Pending
            && catalog.records()[0].pending_revision() == Some(1)
    ));

    let policy_id = PolicyId::new("policy-a");
    let Ok(policy_id) = policy_id else {
        panic!("测试策略 ID 创建失败: {policy_id:?}");
    };
    if let Err(error) = gateway.delete_policy(policy_id, 1).await {
        panic!("待确认策略删除草稿保存失败: {error}");
    }
    let removed = gateway.list_runtime_policies().await;
    assert!(matches!(
        removed,
        Ok(records) if records.len() == 1
            && records[0].state() == RuntimePolicyState::PendingRemoval
            && records[0].target_snapshot_version().is_none()
            && records[0].effective_revision().is_none()
            && records[0].pending_revision() == Some(1)
    ));
}

#[tokio::test]
async fn learning_confirmation_reuses_rules_stages_once_and_replays() {
    let gateway = gateway();
    if let Err(error) = gateway
        .save_policy(site_policy("policy-existing", "example.com", 1), None)
        .await
    {
        panic!("已有直连规则保存失败: {error}");
    }
    let session_id = stopped_site_learning(&gateway).await;
    let confirmation_id = confirmation_id("confirmation-a");
    let domains = selected_domains();

    let confirmed = gateway
        .confirm_learning_candidates(session_id.clone(), confirmation_id.clone(), domains.clone())
        .await;
    let Ok(confirmed) = confirmed else {
        panic!("候选确认失败: {confirmed:?}");
    };
    assert!(!confirmed.replayed());
    assert!(confirmed.snapshot_staged());
    assert_eq!(confirmed.snapshot().artifact().snapshot_version(), 1);
    assert!(confirmed.receipt().policies().iter().any(|value| {
        value.domain().as_ascii() == "example.com"
            && value.policy_id().as_str() == "policy-existing"
    }));
    let policies = gateway.list_policies().await;
    assert!(matches!(policies, Ok(value) if value.len() == 2));

    let replay = gateway
        .confirm_learning_candidates(session_id, confirmation_id, domains)
        .await;
    let Ok(replay) = replay else {
        panic!("候选确认重放失败: {replay:?}");
    };
    assert!(replay.replayed());
    assert!(!replay.snapshot_staged());
    assert_eq!(replay.receipt().policies(), confirmed.receipt().policies());
    let policies = gateway.list_policies().await;
    assert!(matches!(policies, Ok(value) if value.len() == 2));
}

#[tokio::test]
async fn pending_snapshot_blocks_confirmation_before_policy_write() {
    let gateway = gateway();
    if let Err(error) = gateway
        .save_policy(site_policy("policy-existing", "other.example", 1), None)
        .await
    {
        panic!("测试规则保存失败: {error}");
    }
    if let Err(error) = gateway.compile_and_stage().await {
        panic!("测试待确认快照创建失败: {error}");
    }
    let session_id = stopped_site_learning(&gateway).await;

    let result = gateway
        .confirm_learning_candidates(
            session_id,
            confirmation_id("confirmation-pending"),
            selected_domains(),
        )
        .await;

    assert!(matches!(
        result,
        Err(GatewayError::Storage(StorageError::PendingSnapshotExists))
    ));
    let policies = gateway.list_policies().await;
    assert!(matches!(
        policies,
        Ok(value) if value.len() == 1
            && value[0].id().as_str() == "policy-existing"
    ));
}

#[tokio::test]
async fn interrupted_confirmation_refuses_a_changed_policy_catalog() {
    let (gateway, session_id) = gateway_with_interrupted_confirmation();

    let result = gateway
        .confirm_learning_candidates(
            session_id,
            confirmation_id("confirmation-interrupted"),
            selected_domains(),
        )
        .await;

    assert!(matches!(
        result,
        Err(GatewayError::Storage(
            StorageError::LearningConfirmationReplayMismatch
        ))
    ));
    let status = gateway.status().await;
    assert!(matches!(status, Ok(value) if value.pending.is_none()));
}

fn gateway_with_interrupted_confirmation() -> (Gateway, LearningSessionId) {
    let database = PolicyDatabase::open_in_memory(1);
    let Ok(mut database) = database else {
        panic!("中断恢复测试数据库打开失败: {database:?}");
    };
    let session_id = LearningSessionId::new("learning-interrupted");
    let subject = DomainName::normalize("example.com").map(LearningSubject::Site);
    let context = BrowserContextId::new("browser-context-a");
    let (Ok(session_id), Ok(subject), Ok(context)) = (session_id, subject, context) else {
        panic!("中断恢复测试输入无效");
    };
    let session = LearningSession::start(session_id.clone(), subject, Some(context), 1_000, 60_000);
    let Ok(session) = session else {
        panic!("中断恢复学习会话创建失败: {session:?}");
    };
    if let Err(error) = database.learning().start(&session) {
        panic!("中断恢复学习会话保存失败: {error}");
    }
    for (id, domain, kind) in [
        (
            "observation-main",
            "example.com",
            LearningObservationKind::MainFrame,
        ),
        (
            "observation-api",
            "api.example.com",
            LearningObservationKind::Subresource,
        ),
    ] {
        let observation = learning_observation(session_id.clone(), id, domain, kind);
        if let Err(error) = database.learning().record_observation(&observation, 2_000) {
            panic!("中断恢复学习观测保存失败: {error}");
        }
    }
    if let Err(error) = database.learning().stop(&session_id, 3_000) {
        panic!("中断恢复学习会话停止失败: {error}");
    }
    let selections = vec![
        selection("policy-main", "example.com"),
        selection("policy-api", "api.example.com"),
    ];
    if let Err(error) = database.learning_confirmations().confirm_site(
        &confirmation_id("confirmation-interrupted"),
        &session_id,
        &selections,
        4_000,
    ) {
        panic!("中断恢复确认收据创建失败: {error}");
    }
    let removed = PolicyId::new("policy-api");
    let Ok(removed) = removed else {
        panic!("中断恢复删除策略 ID 无效: {removed:?}");
    };
    if let Err(error) = database.policies().delete(&removed, 1, 5_000) {
        panic!("中断恢复测试策略删除失败: {error}");
    }
    (
        Gateway::new(database, CompileCapabilities::full()),
        session_id,
    )
}

fn selection(id: &str, domain: &str) -> LearningPolicySelection {
    let normalized = DomainName::normalize(domain);
    let Ok(normalized) = normalized else {
        panic!("中断恢复测试域名无效: {normalized:?}");
    };
    LearningPolicySelection::new(normalized, site_policy(id, domain, 1), false)
}

async fn stopped_site_learning(gateway: &Gateway) -> LearningSessionId {
    let subject = DomainName::normalize("example.com").map(LearningSubject::Site);
    let context = BrowserContextId::new("browser-context-a");
    let (Ok(subject), Ok(context)) = (subject, context) else {
        panic!("学习测试输入无效");
    };
    let started = gateway.start_learning(subject, Some(context), 60_000).await;
    let Ok(started) = started else {
        panic!("学习会话启动失败: {started:?}");
    };
    for (id, domain, kind) in [
        (
            "observation-main",
            "example.com",
            LearningObservationKind::MainFrame,
        ),
        (
            "observation-api",
            "api.example.com",
            LearningObservationKind::Subresource,
        ),
    ] {
        let observation = learning_observation(started.id().clone(), id, domain, kind);
        if let Err(error) = gateway.record_learning_observation(observation).await {
            panic!("学习观测保存失败: {error}");
        }
    }
    let session_id = started.id().clone();
    if let Err(error) = gateway.stop_learning(session_id.clone()).await {
        panic!("学习会话停止失败: {error}");
    }
    session_id
}

fn learning_observation(
    session_id: LearningSessionId,
    id: &str,
    domain: &str,
    kind: LearningObservationKind,
) -> LearningObservation {
    let observation_id = ObservationId::new(id);
    let context = BrowserContextId::new("browser-context-a");
    let domain = DomainName::normalize(domain);
    let initiator = DomainName::normalize("example.com");
    let (Ok(observation_id), Ok(context), Ok(domain), Ok(initiator)) =
        (observation_id, context, domain, initiator)
    else {
        panic!("学习观测测试输入无效");
    };
    LearningObservation::new(
        session_id,
        observation_id,
        Some(context),
        kind,
        domain,
        Some(initiator),
        LearningResourceType::Fetch,
        false,
    )
}

fn selected_domains() -> Vec<DomainName> {
    ["example.com", "api.example.com"]
        .into_iter()
        .map(|value| {
            DomainName::normalize(value).unwrap_or_else(|error| panic!("测试域名无效: {error}"))
        })
        .collect()
}

fn confirmation_id(value: &str) -> ConfirmationId {
    ConfirmationId::new(value).unwrap_or_else(|error| panic!("测试确认 ID 无效: {error}"))
}

fn gateway() -> Gateway {
    let database = PolicyDatabase::open_in_memory(1);
    let Ok(database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    Gateway::new(database, CompileCapabilities::full())
}

fn proxy_outbound(id: &str) -> OutboundReference {
    let id = OutboundId::new(id);
    let Ok(id) = id else {
        panic!("测试出口 ID 无效: {id:?}");
    };
    let outbound = OutboundReference::new(
        id,
        OutboundKind::Socks5,
        Some("127.0.0.1"),
        Some(1080),
        None,
        1,
    );
    let Ok(outbound) = outbound else {
        panic!("测试出口无效: {outbound:?}");
    };
    outbound
}

fn mark_outbound_ready(gateway: &Gateway, outbound: &OutboundReference) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or_else(|| panic!("测试系统时间无效"));
    if let Err(error) = gateway.report_outbound_health(
        outbound.id().clone(),
        outbound.revision(),
        RuntimeState::Ready,
        Some(20),
        now,
    ) {
        panic!("测试出口健康状态保存失败: {error}");
    }
}

async fn activate(gateway: &Gateway, snapshot: &nonproxy_gatewayd::PublishedSnapshot) {
    let providers = vec!["transparent-proxy".to_owned(), "dns-proxy".to_owned()];
    for provider in &providers {
        let ack = ProviderAck::loaded(
            provider,
            snapshot.artifact().snapshot_version(),
            *snapshot.artifact().content_hash(),
            1_000,
        );
        let Ok(ack) = ack else {
            panic!("测试 Provider ACK 创建失败: {ack:?}");
        };
        if let Err(error) = gateway
            .acknowledge_provider_snapshot(
                snapshot.artifact().snapshot_version(),
                ack,
                providers.clone(),
            )
            .await
        {
            panic!("测试快照激活失败: {error}");
        }
    }
}

fn site_policy(id: &str, domain: &str, revision: u64) -> Policy {
    site_policy_with_decision(id, domain, revision, DecisionSpec::direct())
}

fn site_policy_with_decision(
    id: &str,
    domain: &str,
    revision: u64,
    decision: DecisionSpec,
) -> Policy {
    let matcher = DomainMatcher::new(DomainMatchKind::Exact, domain).and_then(|domain| {
        PolicyMatch::new(None, Some(domain), None, None, Vec::new(), Vec::new())
    });
    let Ok(matcher) = matcher else {
        panic!("测试域名匹配器创建失败: {matcher:?}");
    };
    let id = PolicyId::new(id);
    let Ok(id) = id else {
        panic!("测试策略 ID 创建失败: {id:?}");
    };
    let policy = Policy::new(
        id,
        "直连网站",
        matcher,
        decision,
        PolicyMetadata::new(PolicySourceKind::Site, 100, PolicyOrigin::User, revision),
    );
    let Ok(policy) = policy else {
        panic!("测试策略创建失败: {policy:?}");
    };
    policy
}
