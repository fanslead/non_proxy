mod support;

use nonproxy_model::PolicyId;
use nonproxy_storage::{PolicyDatabase, StorageError};
use support::must_policy;

#[test]
fn policy_round_trip_preserves_normalized_matcher_and_metadata() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let policy = must_policy(1, "初始策略");
    if let Err(error) = database.policies().save(&policy, None, 1_100) {
        panic!("策略保存失败: {error}");
    }
    let loaded = database.policies().get(policy.id());
    let Ok(Some(loaded)) = loaded else {
        panic!("策略读取失败: {loaded:?}");
    };

    assert_eq!(loaded, policy);
    let policies = database.policies().list();
    let Ok(policies) = policies else {
        panic!("策略列表读取失败: {policies:?}");
    };
    assert_eq!(policies, vec![policy]);
}

#[test]
fn optimistic_revision_rejects_stale_writer_without_partial_change() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let initial = must_policy(1, "初始策略");
    if let Err(error) = database.policies().save(&initial, None, 1_100) {
        panic!("初始策略保存失败: {error}");
    }
    let updated = must_policy(2, "更新策略");
    if let Err(error) = database.policies().save(&updated, Some(1), 1_200) {
        panic!("策略更新失败: {error}");
    }
    let stale = must_policy(2, "陈旧覆盖");
    let stale_result = database.policies().save(&stale, Some(1), 1_300);

    assert!(matches!(
        stale_result,
        Err(StorageError::PolicyRevisionConflict)
    ));
    let loaded = database.policies().get(updated.id());
    let Ok(Some(loaded)) = loaded else {
        panic!("更新后策略读取失败: {loaded:?}");
    };
    assert_eq!(loaded, updated);
}

#[test]
fn delete_requires_the_current_revision() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let policy = must_policy(1, "待删除策略");
    if let Err(error) = database.policies().save(&policy, None, 1_100) {
        panic!("策略保存失败: {error}");
    }
    let stale = database.policies().delete(policy.id(), 2, 1_200);
    assert!(matches!(stale, Err(StorageError::PolicyRevisionConflict)));
    if let Err(error) = database.policies().delete(policy.id(), 1, 1_300) {
        panic!("策略删除失败: {error}");
    }
    let id = match PolicyId::new("policy-app-site") {
        Ok(value) => value,
        Err(error) => panic!("测试策略标识创建失败: {error}"),
    };

    assert!(matches!(database.policies().get(&id), Ok(None)));
}

#[test]
fn catalog_generation_changes_only_after_committed_mutation() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    assert!(matches!(database.policies().catalog_generation(), Ok(0)));
    let policy = must_policy(1, "目录代数策略");
    if let Err(error) = database.policies().save(&policy, None, 1_100) {
        panic!("目录代数策略保存失败: {error}");
    }
    assert!(matches!(database.policies().catalog_generation(), Ok(1)));
    let stale = database.policies().delete(policy.id(), 9, 1_200);
    assert!(matches!(stale, Err(StorageError::PolicyRevisionConflict)));
    assert!(matches!(database.policies().catalog_generation(), Ok(1)));
    if let Err(error) = database.policies().delete(policy.id(), 1, 1_300) {
        panic!("目录代数策略删除失败: {error}");
    }
    assert!(matches!(database.policies().catalog_generation(), Ok(2)));
}
