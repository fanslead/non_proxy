use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1::{
        AppIdentity, Destination, EvidenceLevel, FailureMode, IpFamily, Platform, RouteAction,
        TransportProtocol,
    },
    policy::v1::{Decision, DecisionSpec},
    provider::v1::{ConnectionContext, DecisionEvidence, DecisionRecord as ProtoDecisionRecord},
};
use nonproxy_storage::{EvidenceLevel as StoredEvidenceLevel, PolicyDatabase, StorageError};

use crate::{Gateway, GatewayError};

#[tokio::test]
async fn authoritative_recomputation_accepts_idempotent_decision_evidence() {
    let gateway = staged_gateway().await;
    let record = direct_record("flow-1", 1_000);

    let first = gateway
        .ingest_decision_batch("transparent-proxy".to_owned(), 1, vec![record.clone()])
        .await;
    let replay = gateway
        .ingest_decision_batch("transparent-proxy".to_owned(), 1, vec![record])
        .await;
    let listed = gateway.list_connection_decisions(10, 0).await;

    assert!(matches!(first, Ok(1)));
    assert!(matches!(replay, Ok(1)));
    let Ok((records, total)) = listed else {
        panic!("决策证据读取失败: {listed:?}");
    };
    assert_eq!(total, 1);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].evidence_level(), StoredEvidenceLevel::Decision);
    assert_eq!(records[0].destination(), "example.com");
}

#[tokio::test]
async fn rejects_forged_decision_and_preserves_empty_batch() {
    let gateway = staged_gateway().await;
    let mut forged = direct_record("flow-forged", 1_000);
    let Some(decision) = forged.decision.as_mut() else {
        panic!("测试决策缺失");
    };
    decision.result = Some(DecisionSpec {
        action: RouteAction::Block as i32,
        outbound_id: String::new(),
        failure_mode: FailureMode::Closed as i32,
    });

    let result = gateway
        .ingest_decision_batch("transparent-proxy".to_owned(), 1, vec![forged])
        .await;
    let listed = gateway.list_connection_decisions(10, 0).await;

    assert!(matches!(result, Err(GatewayError::InvalidRequest(_))));
    assert!(matches!(listed, Ok((records, 0)) if records.is_empty()));
}

#[tokio::test]
async fn rejects_events_far_in_the_future() {
    let gateway = staged_gateway().await;
    let mut future = direct_record("flow-future", 1_000);
    let Some(context) = future.context.as_mut() else {
        panic!("测试连接上下文缺失");
    };
    context.observed_at = Some(prost_types::Timestamp {
        seconds: 4_000_000_000,
        nanos: 0,
    });

    let result = gateway
        .ingest_decision_batch("transparent-proxy".to_owned(), 1, vec![future])
        .await;

    assert!(matches!(result, Err(GatewayError::InvalidRequest(_))));
}

#[tokio::test]
async fn conflicting_replay_rolls_back_the_whole_batch() {
    let gateway = staged_gateway().await;
    let original = direct_record("flow-stable", 1_000);
    if let Err(error) = gateway
        .ingest_decision_batch("transparent-proxy".to_owned(), 1, vec![original])
        .await
    {
        panic!("初始决策入库失败: {error}");
    }
    let changed = direct_record("flow-stable", 2_000);
    let second = direct_record("flow-second", 2_100);

    let result = gateway
        .ingest_decision_batch("transparent-proxy".to_owned(), 1, vec![changed, second])
        .await;
    let listed = gateway.list_connection_decisions(10, 0).await;

    assert!(matches!(
        result,
        Err(GatewayError::Storage(
            StorageError::ConnectionDecisionReplayMismatch
        ))
    ));
    assert!(matches!(listed, Ok((records, 1)) if records[0].event_id().ends_with("flow-stable")));
}

async fn staged_gateway() -> Gateway {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("决策测试数据库打开失败: {error}"),
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    if let Err(error) = gateway.compile_and_stage().await {
        panic!("决策测试快照暂存失败: {error}");
    }
    gateway
}

fn direct_record(flow_id: &str, observed_at_unix_ms: u64) -> ProtoDecisionRecord {
    ProtoDecisionRecord {
        context: Some(ConnectionContext {
            flow_id: flow_id.to_owned(),
            app: Some(AppIdentity {
                platform: Platform::Macos as i32,
                stable_id: "com.example.browser".to_owned(),
                display_name: "Example Browser".to_owned(),
                ..Default::default()
            }),
            destination: Some(Destination {
                hostname: "example.com".to_owned(),
                normalized_domain: "example.com".to_owned(),
                port: 443,
                transport: TransportProtocol::Tcp as i32,
                ip_family: IpFamily::Unspecified as i32,
                ..Default::default()
            }),
            network_profile_id: String::new(),
            observed_at: Some(prost_types::Timestamp {
                seconds: i64::try_from(observed_at_unix_ms / 1_000)
                    .unwrap_or_else(|_| panic!("测试时间戳超出范围")),
                nanos: i32::try_from((observed_at_unix_ms % 1_000) * 1_000_000)
                    .unwrap_or_else(|_| panic!("测试纳秒超出范围")),
            }),
        }),
        decision: Some(Decision {
            result: Some(DecisionSpec {
                action: RouteAction::Direct as i32,
                outbound_id: String::new(),
                failure_mode: FailureMode::Closed as i32,
            }),
            snapshot_version: 1,
            reason_code: "NP_POLICY_DEFAULT".to_owned(),
            ..Default::default()
        }),
        evidence: Some(DecisionEvidence {
            level: EvidenceLevel::Decision as i32,
            ..Default::default()
        }),
        decision_latency: Some(prost_types::Duration {
            seconds: 0,
            nanos: 25_000,
        }),
        error: None,
    }
}
