use std::net::Ipv4Addr;

use nonproxy_model::{
    AppIdentity, Decision, DecisionSpec, Destination, FailureMode, OutboundId, Platform,
    RouteAction, Transport,
};
use nonproxy_storage::{
    ConnectionDecisionInput, DecisionEvidence, EvidenceLevel, PolicyDatabase, StorageError,
};

#[test]
fn decision_batch_is_idempotent_and_lists_newest_first() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("决策仓储测试数据库打开失败: {database:?}");
    };
    let older = direct_input("flow-older", 1_100, "en0");
    let newer = proxy_input("flow-newer", 1_200, EvidenceLevel::Exit);
    let (Ok(older), Ok(newer)) = (older, newer) else {
        panic!("决策仓储测试输入创建失败");
    };

    let saved = database
        .connection_decisions()
        .save_batch(&[older.clone(), newer.clone()]);
    assert!(matches!(saved, Ok(indices) if indices == [0, 1]));
    assert!(matches!(
        database.connection_decisions().save_batch(&[older, newer]),
        Ok(indices) if indices.is_empty()
    ));

    let page = database.connection_decisions().list_recent(1, 0);
    let Ok((page, total)) = page else {
        panic!("决策仓储第一页读取失败: {page:?}");
    };
    assert_eq!(total, 2);
    let newest = page.first();
    assert!(matches!(
        newest,
        Some(record)
            if record.destination() == "api.example.com"
                && record.action() == RouteAction::Proxy
                && record.evidence_level() == EvidenceLevel::Exit
                && record.outbound_id() == Some("primary")
                && record.exit_probe_id() == Some("probe-1")
    ));

    let page = database.connection_decisions().list_recent(1, 1);
    let Ok((page, total)) = page else {
        panic!("决策仓储第二页读取失败: {page:?}");
    };
    assert_eq!(total, 2);
    assert!(matches!(
        page.as_slice(),
        [record]
            if record.destination() == "198.51.100.10"
                && record.action() == RouteAction::Direct
                && record.interface_name() == Some("en0")
                && record.application() == "Example Bank"
    ));
}

#[test]
fn changed_replay_rolls_back_the_entire_batch() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("决策重放测试数据库打开失败: {database:?}");
    };
    let original = direct_input("same-flow", 1_100, "en0");
    let changed = direct_input("same-flow", 1_101, "en1");
    let additional = direct_input("additional-flow", 1_200, "en0");
    let (Ok(original), Ok(changed), Ok(additional)) = (original, changed, additional) else {
        panic!("决策重放测试输入创建失败");
    };
    if let Err(error) = database.connection_decisions().save_batch(&[original]) {
        panic!("初始决策保存失败: {error}");
    }

    let result = database
        .connection_decisions()
        .save_batch(&[additional, changed]);

    assert!(matches!(
        result,
        Err(StorageError::ConnectionDecisionReplayMismatch)
    ));
    assert!(matches!(
        database.connection_decisions().list_recent(10, 0),
        Ok((records, 1)) if records.len() == 1
    ));
}

#[test]
fn mixed_replay_reports_only_the_new_insert_index() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("混合重放测试数据库打开失败: {database:?}");
    };
    let existing = direct_input("existing-flow", 1_100, "en0")
        .unwrap_or_else(|error| panic!("既有决策夹具失败: {error}"));
    let inserted = direct_input("inserted-flow", 1_200, "en0")
        .unwrap_or_else(|error| panic!("新增决策夹具失败: {error}"));
    if let Err(error) = database
        .connection_decisions()
        .save_batch(std::slice::from_ref(&existing))
    {
        panic!("既有决策写入失败: {error}");
    }

    let result = database
        .connection_decisions()
        .save_batch(&[existing, inserted]);

    assert!(matches!(result, Ok(indices) if indices == [1]));
    assert!(matches!(
        database.connection_decisions().list_recent(10, 0),
        Ok((records, 2)) if records.len() == 2
    ));
}

#[test]
fn evidence_cannot_overclaim_the_selected_route() {
    let proxy_id = outbound_id();
    let decision = Decision::defaulted(proxy_decision(proxy_id.clone()), 1, "NP_POLICY_DEFAULT");
    let app = app();
    let destination = destination(Some("api.example.com"));
    let evidence = DecisionEvidence::new(
        EvidenceLevel::Path,
        Some("en0".to_owned()),
        None,
        None,
        false,
    );
    let (Ok(app), Ok(destination), Ok(evidence)) = (app, destination, evidence) else {
        panic!("证据负向测试夹具创建失败");
    };

    let result = ConnectionDecisionInput::new(
        "transparent-proxy",
        1,
        "wrong-path",
        1_000,
        app,
        destination,
        decision,
        evidence,
        Some(100),
        None,
    );

    assert!(matches!(result, Err(StorageError::DecisionEvidenceInvalid)));
    assert!(
        DecisionEvidence::new(
            EvidenceLevel::Exit,
            Some("en0".to_owned()),
            None,
            None,
            false,
        )
        .is_err()
    );
}

#[test]
fn fail_open_proxy_can_record_the_real_direct_interface() {
    let evidence = DecisionEvidence::new(
        EvidenceLevel::Path,
        Some("ifindex:12".to_owned()),
        None,
        None,
        true,
    );
    let decision = DecisionSpec::new(RouteAction::Proxy, Some(outbound_id()), FailureMode::Open);
    let (Ok(evidence), Ok(decision), Ok(app), Ok(destination)) =
        (evidence, decision, app(), destination(None))
    else {
        panic!("fail-open 证据测试夹具创建失败");
    };
    let input = ConnectionDecisionInput::new(
        "windows-wfp",
        3,
        "fallback-flow",
        1_300,
        app,
        destination,
        Decision::defaulted(decision, 1, "NP_POLICY_DEFAULT"),
        evidence,
        Some(900),
        Some("NP_PROXY_FAIL_OPEN_DIRECT".to_owned()),
    );
    let Ok(input) = input else {
        panic!("合法 fail-open 决策输入被拒绝: {input:?}");
    };
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("fail-open 测试数据库打开失败: {database:?}");
    };

    assert!(matches!(
        database.connection_decisions().save_batch(&[input]),
        Ok(indices) if indices == [0]
    ));
    assert!(matches!(
        database.connection_decisions().list_recent(10, 0),
        Ok((records, 1))
            if records[0].fail_open_direct()
                && records[0].interface_name() == Some("ifindex:12")
                && records[0].error_code() == Some("NP_PROXY_FAIL_OPEN_DIRECT")
    ));
}

#[test]
fn fail_open_evidence_requires_open_policy_and_a_reason() {
    let evidence = || {
        DecisionEvidence::new(
            EvidenceLevel::Path,
            Some("ifindex:12".to_owned()),
            None,
            None,
            true,
        )
    };
    let closed = ConnectionDecisionInput::new(
        "windows-wfp",
        3,
        "closed-flow",
        1_300,
        app().unwrap_or_else(|error| panic!("应用夹具失败: {error}")),
        destination(None).unwrap_or_else(|error| panic!("目标夹具失败: {error}")),
        Decision::defaulted(proxy_decision(outbound_id()), 1, "NP_POLICY_DEFAULT"),
        evidence().unwrap_or_else(|error| panic!("证据夹具失败: {error}")),
        None,
        Some("NP_PROXY_FAIL_OPEN_DIRECT".to_owned()),
    );
    let open_without_reason = ConnectionDecisionInput::new(
        "windows-wfp",
        3,
        "missing-reason-flow",
        1_300,
        app().unwrap_or_else(|error| panic!("应用夹具失败: {error}")),
        destination(None).unwrap_or_else(|error| panic!("目标夹具失败: {error}")),
        Decision::defaulted(
            DecisionSpec::new(RouteAction::Proxy, Some(outbound_id()), FailureMode::Open)
                .unwrap_or_else(|error| panic!("决策夹具失败: {error}")),
            1,
            "NP_POLICY_DEFAULT",
        ),
        evidence().unwrap_or_else(|error| panic!("证据夹具失败: {error}")),
        None,
        None,
    );

    assert!(matches!(closed, Err(StorageError::DecisionEvidenceInvalid)));
    assert!(matches!(
        open_without_reason,
        Err(StorageError::DecisionEvidenceInvalid)
    ));
}

fn direct_input(
    flow_id: &str,
    occurred_at_unix_ms: u64,
    interface_name: &str,
) -> Result<ConnectionDecisionInput, StorageError> {
    let app = app()?;
    let destination = destination(None)?;
    let evidence = DecisionEvidence::new(
        EvidenceLevel::Path,
        Some(interface_name.to_owned()),
        None,
        None,
        false,
    )?;
    ConnectionDecisionInput::new(
        "transparent-proxy",
        1,
        flow_id,
        occurred_at_unix_ms,
        app,
        destination,
        Decision::defaulted(DecisionSpec::direct(), 1, "NP_POLICY_DEFAULT"),
        evidence,
        Some(500),
        None,
    )
}

fn proxy_input(
    flow_id: &str,
    occurred_at_unix_ms: u64,
    level: EvidenceLevel,
) -> Result<ConnectionDecisionInput, StorageError> {
    let outbound_id = outbound_id();
    let app = app()?;
    let destination = destination(Some("api.example.com"))?;
    let evidence = DecisionEvidence::new(
        level,
        None,
        Some(outbound_id.clone()),
        (level == EvidenceLevel::Exit).then(|| "probe-1".to_owned()),
        false,
    )?;
    ConnectionDecisionInput::new(
        "transparent-proxy",
        1,
        flow_id,
        occurred_at_unix_ms,
        app,
        destination,
        Decision::defaulted(proxy_decision(outbound_id), 1, "NP_POLICY_DEFAULT"),
        evidence,
        Some(750),
        None,
    )
}

fn app() -> Result<AppIdentity, StorageError> {
    AppIdentity::new(Platform::MacOs, "com.example.bank")
        .and_then(|value| value.with_display_name("Example Bank"))
        .map_err(StorageError::from)
}

fn destination(hostname: Option<&str>) -> Result<Destination, StorageError> {
    Destination::new(
        hostname,
        (hostname.is_none()).then_some(Ipv4Addr::new(198, 51, 100, 10).into()),
        443,
        Transport::Tcp,
    )
    .map_err(StorageError::from)
}

fn outbound_id() -> OutboundId {
    match OutboundId::new("primary") {
        Ok(value) => value,
        Err(error) => panic!("测试出口 ID 创建失败: {error}"),
    }
}

fn proxy_decision(outbound_id: OutboundId) -> DecisionSpec {
    match DecisionSpec::new(RouteAction::Proxy, Some(outbound_id), FailureMode::Closed) {
        Ok(value) => value,
        Err(error) => panic!("测试代理决策创建失败: {error}"),
    }
}
