use nonproxy_model::{
    AppMatcher, DecisionSpec, FailureMode, OutboundGroupId, OutboundId, Platform, Policy, PolicyId,
    PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind,
};
use nonproxy_storage::{
    MAXIMUM_OUTBOUND_GROUP_MEMBERS, OutboundGroup, OutboundGroupStrategy, OutboundKind,
    OutboundReference, PolicyDatabase, StorageError,
};

#[test]
fn group_round_trip_preserves_priority_and_revision_cas() {
    let mut database = database_with_connectable_outbounds();
    let initial = group("daily", "日常自动切换", &["secondary", "primary"], 1);
    database
        .outbound_groups()
        .save(&initial, None, 1_100)
        .unwrap_or_else(|error| panic!("出口组首次保存失败: {error}"));

    let loaded = database
        .outbound_groups()
        .get(initial.id())
        .unwrap_or_else(|error| panic!("出口组读取失败: {error}"))
        .unwrap_or_else(|| panic!("已保存出口组不存在"));
    assert_eq!(loaded, initial);
    assert_eq!(
        loaded
            .members()
            .iter()
            .map(OutboundId::as_str)
            .collect::<Vec<_>>(),
        vec!["secondary", "primary"]
    );

    let updated = group("daily", "日常故障切换", &["primary", "secondary"], 2);
    database
        .outbound_groups()
        .save(&updated, Some(1), 1_200)
        .unwrap_or_else(|error| panic!("出口组更新失败: {error}"));
    assert!(matches!(
        database.outbound_groups().save(&updated, Some(1), 1_300),
        Err(StorageError::OutboundGroupRevisionConflict)
    ));
    assert!(matches!(
        database.outbound_groups().list().as_deref(),
        Ok([value]) if value == &updated
    ));
}

#[test]
fn group_shape_rejects_ambiguous_or_unbounded_membership() {
    let first = outbound_id("first");
    let second = outbound_id("second");
    let id = group_id("invalid");

    assert!(matches!(
        OutboundGroup::new(
            id.clone(),
            "只有一个",
            OutboundGroupStrategy::Failover,
            vec![first.clone()],
            1,
        ),
        Err(StorageError::OutboundGroupInvalid)
    ));
    assert!(matches!(
        OutboundGroup::new(
            id.clone(),
            "重复成员",
            OutboundGroupStrategy::Failover,
            vec![first.clone(), first],
            1,
        ),
        Err(StorageError::OutboundGroupInvalid)
    ));
    assert!(matches!(
        OutboundGroup::new(
            id.clone(),
            " 名称含歧义空白",
            OutboundGroupStrategy::Failover,
            vec![outbound_id("first"), second.clone()],
            1,
        ),
        Err(StorageError::OutboundGroupInvalid)
    ));
    let too_many = (0..=MAXIMUM_OUTBOUND_GROUP_MEMBERS)
        .map(|index| outbound_id(&format!("member-{index}")))
        .collect();
    assert!(matches!(
        OutboundGroup::new(id, "成员过多", OutboundGroupStrategy::Failover, too_many, 1,),
        Err(StorageError::OutboundGroupInvalid)
    ));
}

#[test]
fn group_save_rejects_missing_or_non_connectable_members_atomically() {
    let mut database = PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("出口组校验数据库打开失败: {error}"));
    let primary = outbound("primary", OutboundKind::Socks5);
    database
        .outbounds()
        .save(&primary, None, 1_050)
        .unwrap_or_else(|error| panic!("出口组主成员保存失败: {error}"));
    let missing = group("missing-member", "缺少成员", &["primary", "absent"], 1);

    assert!(matches!(
        database.outbound_groups().save(&missing, None, 1_100),
        Err(StorageError::OutboundGroupMemberNotFound)
    ));
    assert!(matches!(
        database.outbound_groups().get(missing.id()),
        Ok(None)
    ));

    let adapter = outbound("adapter", OutboundKind::Adapter);
    database
        .outbounds()
        .save(&adapter, None, 1_150)
        .unwrap_or_else(|error| panic!("适配器出口保存失败: {error}"));
    let unsupported = group(
        "unsupported-member",
        "不支持成员",
        &["primary", "adapter"],
        1,
    );
    assert!(matches!(
        database.outbound_groups().save(&unsupported, None, 1_200),
        Err(StorageError::OutboundGroupMemberUnsupported)
    ));
    assert!(matches!(
        database.outbound_groups().get(unsupported.id()),
        Ok(None)
    ));
}

#[test]
fn group_delete_is_revision_guarded_and_keeps_member_outbounds() {
    let mut database = database_with_connectable_outbounds();
    let group = group("temporary", "临时故障切换", &["primary", "secondary"], 1);
    database
        .outbound_groups()
        .save(&group, None, 1_100)
        .unwrap_or_else(|error| panic!("待删除出口组保存失败: {error}"));

    assert!(matches!(
        database.outbound_groups().delete(group.id(), 9, 1_200),
        Err(StorageError::OutboundGroupRevisionConflict)
    ));
    assert!(matches!(
        database.outbound_groups().get(group.id()),
        Ok(Some(_))
    ));

    database
        .outbound_groups()
        .delete(group.id(), 1, 1_300)
        .unwrap_or_else(|error| panic!("出口组删除失败: {error}"));
    assert!(matches!(
        database.outbound_groups().get(group.id()),
        Ok(None)
    ));
    assert_eq!(
        database
            .outbounds()
            .list()
            .unwrap_or_else(|error| panic!("删除出口组后成员读取失败: {error}"))
            .len(),
        2
    );
}

#[test]
fn policy_group_target_round_trip_blocks_group_deletion_until_policy_is_removed() {
    let mut database = database_with_connectable_outbounds();
    let group = group(
        "policy-target",
        "策略故障切换",
        &["primary", "secondary"],
        1,
    );
    database
        .outbound_groups()
        .save(&group, None, 1_100)
        .unwrap_or_else(|error| panic!("策略目标组保存失败: {error}"));
    let policy = group_policy(group.id().clone());
    database
        .policies()
        .save(&policy, None, 1_200)
        .unwrap_or_else(|error| panic!("出口组策略保存失败: {error}"));

    let loaded = database
        .policies()
        .get(policy.id())
        .unwrap_or_else(|error| panic!("出口组策略读取失败: {error}"))
        .unwrap_or_else(|| panic!("已保存出口组策略不存在"));
    assert_eq!(loaded.decision().outbound_group_id(), Some(group.id()));
    assert!(loaded.decision().outbound_id().is_none());
    assert!(matches!(
        database.outbound_groups().delete(group.id(), 9, 1_250),
        Err(StorageError::OutboundGroupRevisionConflict)
    ));
    assert!(matches!(
        database.outbound_groups().delete(group.id(), 1, 1_300),
        Err(StorageError::OutboundGroupInUse)
    ));

    database
        .policies()
        .delete(policy.id(), 1, 1_400)
        .unwrap_or_else(|error| panic!("出口组策略删除失败: {error}"));
    database
        .outbound_groups()
        .delete(group.id(), 1, 1_500)
        .unwrap_or_else(|error| panic!("解除策略引用后的出口组删除失败: {error}"));
}

fn database_with_connectable_outbounds() -> PolicyDatabase {
    let mut database = PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("出口组测试数据库打开失败: {error}"));
    for (index, id) in ["primary", "secondary"].into_iter().enumerate() {
        let outbound = outbound(id, OutboundKind::Socks5);
        database
            .outbounds()
            .save(
                &outbound,
                None,
                1_010 + u64::try_from(index).unwrap_or_default(),
            )
            .unwrap_or_else(|error| panic!("出口组成员保存失败: {error}"));
    }
    database
}

fn outbound(id: &str, kind: OutboundKind) -> OutboundReference {
    let (host, port) = if kind == OutboundKind::Adapter {
        (None, None)
    } else {
        (Some("proxy.example"), Some(1_080))
    };
    OutboundReference::new(outbound_id(id), kind, host, port, None, 1)
        .unwrap_or_else(|error| panic!("出口组测试出口创建失败: {error}"))
}

fn group(id: &str, display_name: &str, members: &[&str], revision: u64) -> OutboundGroup {
    OutboundGroup::new(
        group_id(id),
        display_name,
        OutboundGroupStrategy::Failover,
        members.iter().map(|value| outbound_id(value)).collect(),
        revision,
    )
    .unwrap_or_else(|error| panic!("出口组测试配置创建失败: {error}"))
}

fn group_policy(group_id: OutboundGroupId) -> Policy {
    let app = AppMatcher::new(Platform::MacOs, "com.example.group")
        .unwrap_or_else(|error| panic!("出口组策略应用匹配器无效: {error}"));
    let matcher = PolicyMatch::new(Some(app), None, None, None, Vec::new(), Vec::new())
        .unwrap_or_else(|error| panic!("出口组策略匹配器无效: {error}"));
    let decision = DecisionSpec::proxy_group(group_id, FailureMode::Closed)
        .unwrap_or_else(|error| panic!("出口组策略决策无效: {error}"));
    Policy::new(
        PolicyId::new("group-policy").unwrap_or_else(|error| panic!("出口组策略标识无效: {error}")),
        "出口组策略",
        matcher,
        decision,
        PolicyMetadata::new(PolicySourceKind::App, 100, PolicyOrigin::User, 1),
    )
    .unwrap_or_else(|error| panic!("出口组策略无效: {error}"))
}

fn outbound_id(value: &str) -> OutboundId {
    OutboundId::new(value).unwrap_or_else(|error| panic!("出口标识创建失败: {error}"))
}

fn group_id(value: &str) -> OutboundGroupId {
    OutboundGroupId::new(value).unwrap_or_else(|error| panic!("出口组标识创建失败: {error}"))
}
