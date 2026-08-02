use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1::{
        AppIdentity, Destination, EvidenceLevel, FailureMode, IpFamily, PageRequest, Platform,
        RouteAction, TransportProtocol,
    },
    control::v1::ListConnectionDecisionsRequest,
    policy::v1::{Decision, DecisionSpec},
    provider::v1::{ConnectionContext, DecisionEvidence, DecisionRecord as ProtoDecisionRecord},
};
use nonproxy_storage::PolicyDatabase;

use super::{list, parse_page};
use crate::Gateway;

#[test]
fn page_contract_is_bounded_and_rejects_non_numeric_tokens() {
    assert!(matches!(parse_page(None), Ok((100, 0))));
    assert!(
        parse_page(Some(PageRequest {
            page_size: 201,
            page_token: String::new(),
        }))
        .is_err()
    );
    assert!(
        parse_page(Some(PageRequest {
            page_size: 10,
            page_token: "bad".to_owned(),
        }))
        .is_err()
    );
}

#[tokio::test]
async fn list_returns_redacted_path_evidence_and_total_count() {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("活动 RPC 测试数据库打开失败: {error}"),
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    if let Err(error) = gateway.compile_and_stage().await {
        panic!("活动 RPC 测试快照暂存失败: {error}");
    }
    if let Err(error) = gateway
        .ingest_decision_batch(
            "transparent-proxy".to_owned(),
            2,
            vec![direct_path_record()],
        )
        .await
    {
        panic!("活动 RPC 测试决策写入失败: {error}");
    }

    let response = list(
        &gateway,
        ListConnectionDecisionsRequest {
            page: Some(PageRequest {
                page_size: 1,
                page_token: String::new(),
            }),
        },
    )
    .await;

    let Ok(response) = response else {
        panic!("活动 RPC 查询失败: {response:?}");
    };
    let decision = match response.decisions.first() {
        Some(value) => value,
        None => panic!("活动 RPC 必须返回决策记录"),
    };
    assert_eq!(response.total_count, 1);
    assert_eq!(decision.destination, "example.com");
    assert_eq!(decision.evidence_level, EvidenceLevel::Path as i32);
    assert_eq!(decision.interface_name, "en0");
    assert_eq!(decision.provider_generation, 2);
    assert_eq!(decision.app_signer_id, "TEAM-EXAMPLE");
    assert_eq!(decision.app_parent_stable_id, "com.example.parent");
    assert_eq!(decision.app_helper_group_id, "com.example.browser");
    assert!(decision.event_id.ends_with("flow-path"));
}

fn direct_path_record() -> ProtoDecisionRecord {
    ProtoDecisionRecord {
        context: Some(ConnectionContext {
            flow_id: "flow-path".to_owned(),
            app: Some(AppIdentity {
                platform: Platform::Macos as i32,
                stable_id: "com.example.browser".to_owned(),
                signer_id: "TEAM-EXAMPLE".to_owned(),
                display_name: "Example Browser".to_owned(),
                parent_stable_id: "com.example.parent".to_owned(),
                helper_group_id: "com.example.browser".to_owned(),
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
            observed_at: Some(prost_types::Timestamp {
                seconds: 1,
                nanos: 0,
            }),
            ..Default::default()
        }),
        decision: Some(Decision {
            result: Some(DecisionSpec {
                action: RouteAction::Direct as i32,
                failure_mode: FailureMode::Closed as i32,
                outbound_id: String::new(),
                outbound_group_id: String::new(),
            }),
            snapshot_version: 1,
            reason_code: "NP_POLICY_DEFAULT".to_owned(),
            ..Default::default()
        }),
        evidence: Some(DecisionEvidence {
            level: EvidenceLevel::Path as i32,
            interface_name: "en0".to_owned(),
            ..Default::default()
        }),
        decision_latency: Some(prost_types::Duration {
            seconds: 0,
            nanos: 10_000,
        }),
        error: None,
    }
}
