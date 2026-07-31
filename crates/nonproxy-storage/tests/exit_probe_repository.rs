mod support;

use std::net::{IpAddr, Ipv4Addr};

use nonproxy_model::OutboundId;
use nonproxy_storage::{ExitProbeInput, ExitProbeRoute, PolicyDatabase, StorageError};

#[test]
fn signed_receipts_are_idempotent_and_listed_in_verification_order() {
    let mut database = open_database();
    let direct = input(1, ExitProbeRoute::Direct, 1_000);
    let proxy = input(2, ExitProbeRoute::Proxy(outbound("office")), 2_000);

    let direct_sequence = database.exit_probes().save(&direct);
    let replay_sequence = database.exit_probes().save(&direct);
    let proxy_sequence = database.exit_probes().save(&proxy);
    let listed = database.exit_probes().list_recent(10, 0);

    assert!(matches!(
        (direct_sequence, replay_sequence, proxy_sequence),
        (Ok(direct), Ok(replay), Ok(proxy)) if direct == replay && proxy > direct
    ));
    let Ok((records, total)) = listed else {
        panic!("出口回执查询失败: {listed:?}");
    };
    assert_eq!(total, 2);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].probe_id(), token(2, 43));
    assert!(matches!(
        records[0].route(),
        ExitProbeRoute::Proxy(id) if id.as_str() == "office"
    ));
    assert_eq!(records[1].observed_ip(), public_ip());
    assert_eq!(records[1].key_id(), token(1, 22));
}

#[test]
fn conflicting_replay_and_untrusted_evidence_are_rejected() {
    let mut database = open_database();
    let original = input(3, ExitProbeRoute::Direct, 3_000);
    if let Err(error) = database.exit_probes().save(&original) {
        panic!("测试回执保存失败: {error}");
    }
    let conflicting = ExitProbeInput::new(
        token(3, 43),
        ExitProbeRoute::Proxy(outbound("changed")),
        public_ip(),
        3_000,
        token(3, 22),
        3_000,
    );
    let private = ExitProbeInput::new(
        token(4, 43),
        ExitProbeRoute::Direct,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        4_000,
        token(4, 22),
        4_000,
    );
    let stale = ExitProbeInput::new(
        token(5, 43),
        ExitProbeRoute::Direct,
        public_ip(),
        5_000,
        token(5, 22),
        125_001,
    );

    let Ok(conflicting) = conflicting else {
        panic!("冲突回执夹具创建失败: {conflicting:?}");
    };
    assert!(matches!(
        database.exit_probes().save(&conflicting),
        Err(StorageError::ExitProbeReplayMismatch)
    ));
    assert!(matches!(private, Err(StorageError::ExitProbeInvalid)));
    assert!(matches!(stale, Err(StorageError::ExitProbeInvalid)));
    let remaining = database.exit_probes().list_recent(10, 0);
    let Ok((records, 1)) = remaining else {
        panic!("冲突重放后权威回执数量无效: {remaining:?}");
    };
    let Some(record) = records.first() else {
        panic!("冲突重放删除了原始回执");
    };
    assert_eq!(record.probe_id(), token(3, 43));
    assert!(matches!(record.route(), ExitProbeRoute::Direct));
}

#[test]
fn receipt_history_is_bounded_without_dropping_the_newest_result() {
    let mut database = open_database();
    for index in 1_u64..=2_050 {
        let input = ExitProbeInput::new(
            format!("{index:043}"),
            ExitProbeRoute::Direct,
            public_ip(),
            index,
            token(8, 22),
            index,
        )
        .unwrap_or_else(|error| panic!("有界回执夹具创建失败: {error}"));
        database
            .exit_probes()
            .save(&input)
            .unwrap_or_else(|error| panic!("有界回执保存失败: {error}"));
    }

    let listed = database.exit_probes().list_recent(1, 0);
    let oldest = database.exit_probes().list_recent(1, 2_047);

    let Ok((records, total)) = listed else {
        panic!("有界出口回执查询失败: {listed:?}");
    };
    assert_eq!(total, 2_048);
    let latest = records
        .first()
        .unwrap_or_else(|| panic!("缺少最新出口回执"));
    assert_eq!(latest.probe_id(), format!("{:043}", 2_050));
    let Ok((oldest_records, oldest_total)) = oldest else {
        panic!("有界出口回执尾页查询失败: {oldest:?}");
    };
    assert_eq!(oldest_total, 2_048);
    let earliest_retained = oldest_records
        .first()
        .unwrap_or_else(|| panic!("缺少最早保留的出口回执"));
    assert_eq!(earliest_retained.probe_id(), format!("{:043}", 3));
}

fn open_database() -> PolicyDatabase {
    PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("测试数据库创建失败: {error}"))
}

fn input(seed: u8, route: ExitProbeRoute, timestamp: u64) -> ExitProbeInput {
    ExitProbeInput::new(
        token(seed, 43),
        route,
        public_ip(),
        timestamp,
        token(seed, 22),
        timestamp,
    )
    .unwrap_or_else(|error| panic!("出口回执夹具创建失败: {error}"))
}

fn token(seed: u8, length: usize) -> String {
    char::from(b'A' + (seed % 26)).to_string().repeat(length)
}

fn outbound(value: &str) -> OutboundId {
    OutboundId::new(value).unwrap_or_else(|error| panic!("测试出口标识无效: {error}"))
}

fn public_ip() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))
}
