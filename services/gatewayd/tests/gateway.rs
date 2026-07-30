use nonproxy_gatewayd::{Gateway, GatewayError, RuntimePolicyState, decode_snapshot_payload};
use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, Policy, PolicyId, PolicyMatch, PolicyMetadata,
    PolicyOrigin, PolicySourceKind,
};
use nonproxy_policy_compiler::{CompileCapabilities, CompileError};
use nonproxy_storage::{PolicyDatabase, StorageError};

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
