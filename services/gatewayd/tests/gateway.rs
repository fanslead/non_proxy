use nonproxy_gatewayd::{Gateway, GatewayError, RuntimePolicyState, decode_snapshot_payload};
use nonproxy_learning::{
    BrowserContextId, ConfirmationId, LearningObservation, LearningObservationKind,
    LearningResourceType, LearningSession, LearningSessionId, LearningSubject, ObservationId,
};
use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, DomainName, Policy, PolicyId, PolicyMatch,
    PolicyMetadata, PolicyOrigin, PolicySourceKind,
};
use nonproxy_policy_compiler::{CompileCapabilities, CompileError};
use nonproxy_storage::{LearningPolicySelection, PolicyDatabase, StorageError};

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

    assert_eq!(policies, vec![policy]);
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

fn site_policy(id: &str, domain: &str, revision: u64) -> Policy {
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
        DecisionSpec::direct(),
        PolicyMetadata::new(PolicySourceKind::Site, 100, PolicyOrigin::User, revision),
    );
    let Ok(policy) = policy else {
        panic!("测试策略创建失败: {policy:?}");
    };
    policy
}
