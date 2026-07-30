mod support;

use nonproxy_storage::{PolicyDatabase, ProviderAck, SnapshotStatus, StorageError};
use support::artifact;

fn providers() -> Vec<String> {
    vec!["transparent-proxy".to_owned(), "dns-proxy".to_owned()]
}

#[test]
fn activation_waits_for_every_required_provider_ack() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let snapshot = artifact(1, 7);
    let Ok(snapshot) = snapshot else {
        panic!("测试快照创建失败: {snapshot:?}");
    };
    if let Err(error) = database.snapshots().stage(&snapshot) {
        panic!("测试快照暂存失败: {error}");
    }
    let first = ProviderAck::loaded("transparent-proxy", 1, *snapshot.content_hash(), 1_100);
    let Ok(first) = first else {
        panic!("首个 ACK 创建失败: {first:?}");
    };
    if let Err(error) = database.snapshots().record_ack(1, &first) {
        panic!("首个 ACK 保存失败: {error}");
    }

    assert!(matches!(
        database.snapshots().activate(1, &providers(), 1_200),
        Err(StorageError::ProviderAcknowledgementMissing)
    ));
    assert!(matches!(database.snapshots().active(), Ok(None)));

    let second = ProviderAck::loaded("dns-proxy", 1, *snapshot.content_hash(), 1_300);
    let Ok(second) = second else {
        panic!("第二个 ACK 创建失败: {second:?}");
    };
    if let Err(error) = database.snapshots().record_ack(1, &second) {
        panic!("第二个 ACK 保存失败: {error}");
    }
    if let Err(error) = database.snapshots().activate(1, &providers(), 1_400) {
        panic!("快照激活失败: {error}");
    }
    let active = database.snapshots().active();
    let Ok(Some(active)) = active else {
        panic!("激活快照读取失败: {active:?}");
    };

    assert_eq!(active.status(), SnapshotStatus::Active);
    assert_eq!(active.artifact(), &snapshot);
}

#[test]
fn rejection_preserves_the_previous_active_snapshot() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    activate_single_provider(&mut database, 1, 1, 1_100);
    let next = artifact(2, 2);
    let Ok(next) = next else {
        panic!("第二快照创建失败: {next:?}");
    };
    if let Err(error) = database.snapshots().stage(&next) {
        panic!("第二快照暂存失败: {error}");
    }
    let rejected = ProviderAck::rejected(
        "transparent-proxy",
        2,
        *next.content_hash(),
        "NP_PLATFORM_SNAPSHOT_REJECTED",
        1_300,
    );
    let Ok(rejected) = rejected else {
        panic!("拒绝 ACK 创建失败: {rejected:?}");
    };
    if let Err(error) = database.snapshots().record_ack(2, &rejected) {
        panic!("拒绝 ACK 保存失败: {error}");
    }
    let active = database.snapshots().active();
    let rejected_record = database.snapshots().get(2);
    let (Ok(Some(active)), Ok(Some(rejected_record))) = (active, rejected_record) else {
        panic!("拒绝后的快照状态读取失败");
    };

    assert_eq!(active.artifact().snapshot_version(), 1);
    assert_eq!(rejected_record.status(), SnapshotStatus::Rejected);
    assert_eq!(
        rejected_record.failure_code(),
        Some("NP_PLATFORM_SNAPSHOT_REJECTED")
    );
}

#[test]
fn rollback_creates_a_new_monotonic_snapshot_version() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    activate_single_provider(&mut database, 1, 3, 1_100);
    let rollback = database.snapshots().stage_rollback(2, 1, 1_200);
    let Ok(rollback) = rollback else {
        panic!("回滚快照暂存失败: {rollback:?}");
    };
    let ack = ProviderAck::loaded("transparent-proxy", 2, *rollback.content_hash(), 1_300);
    let Ok(ack) = ack else {
        panic!("回滚 ACK 创建失败: {ack:?}");
    };
    if let Err(error) = database.snapshots().record_ack(2, &ack) {
        panic!("回滚 ACK 保存失败: {error}");
    }
    if let Err(error) = database
        .snapshots()
        .activate(2, &["transparent-proxy".to_owned()], 1_400)
    {
        panic!("回滚快照激活失败: {error}");
    }
    let active = database.snapshots().active();
    let Ok(Some(active)) = active else {
        panic!("回滚后的 active 快照读取失败: {active:?}");
    };

    assert_eq!(active.artifact().snapshot_version(), 2);
    assert_eq!(active.source_snapshot_version(), Some(1));
    assert_eq!(active.artifact().content_hash(), &[3; 32]);
}

#[test]
fn hash_mismatch_and_non_monotonic_versions_are_rejected() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let first = artifact(1, 1);
    let Ok(first) = first else {
        panic!("首个快照创建失败: {first:?}");
    };
    if let Err(error) = database.snapshots().stage(&first) {
        panic!("首个快照暂存失败: {error}");
    }
    let wrong = ProviderAck::loaded("transparent-proxy", 1, [9; 32], 1_100);
    let Ok(wrong) = wrong else {
        panic!("错误哈希 ACK 创建失败: {wrong:?}");
    };
    assert!(matches!(
        database.snapshots().record_ack(1, &wrong),
        Err(StorageError::SnapshotHashMismatch)
    ));
    if let Err(error) = database
        .snapshots()
        .reject_pending(1, "NP_POLICY_SNAPSHOT_INVALID", 1_200)
    {
        panic!("待发布快照拒绝失败: {error}");
    }
    let duplicate = artifact(1, 2);
    let Ok(duplicate) = duplicate else {
        panic!("重复版本快照创建失败: {duplicate:?}");
    };

    assert!(matches!(
        database.snapshots().stage(&duplicate),
        Err(StorageError::SnapshotVersionNotMonotonic)
    ));
}

#[test]
fn provider_generation_is_monotonic_and_equal_replay_is_idempotent() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let snapshot = artifact(1, 4);
    let Ok(snapshot) = snapshot else {
        panic!("测试快照创建失败: {snapshot:?}");
    };
    if let Err(error) = database.snapshots().stage(&snapshot) {
        panic!("快照暂存失败: {error}");
    }
    let ack = ProviderAck::loaded("transparent-proxy", 5, *snapshot.content_hash(), 1_100);
    let Ok(ack) = ack else {
        panic!("测试 ACK 创建失败: {ack:?}");
    };
    if let Err(error) = database.snapshots().record_ack(1, &ack) {
        panic!("测试 ACK 保存失败: {error}");
    }
    if let Err(error) = database.snapshots().record_ack(1, &ack) {
        panic!("相同 ACK 重放应当幂等: {error}");
    }
    let stale = ProviderAck::loaded("transparent-proxy", 4, *snapshot.content_hash(), 1_200);
    let Ok(stale) = stale else {
        panic!("陈旧 ACK 创建失败: {stale:?}");
    };

    assert!(matches!(
        database.snapshots().record_ack(1, &stale),
        Err(StorageError::SnapshotStateConflict)
    ));
}

fn activate_single_provider(
    database: &mut PolicyDatabase,
    version: u64,
    marker: u8,
    base_time: u64,
) {
    let snapshot = artifact(version, marker);
    let Ok(snapshot) = snapshot else {
        panic!("单 Provider 快照创建失败: {snapshot:?}");
    };
    if let Err(error) = database.snapshots().stage(&snapshot) {
        panic!("单 Provider 快照暂存失败: {error}");
    }
    let ack = ProviderAck::loaded(
        "transparent-proxy",
        version,
        *snapshot.content_hash(),
        base_time,
    );
    let Ok(ack) = ack else {
        panic!("单 Provider ACK 创建失败: {ack:?}");
    };
    if let Err(error) = database.snapshots().record_ack(version, &ack) {
        panic!("单 Provider ACK 保存失败: {error}");
    }
    if let Err(error) =
        database
            .snapshots()
            .activate(version, &["transparent-proxy".to_owned()], base_time + 1)
    {
        panic!("单 Provider 快照激活失败: {error}");
    }
}
