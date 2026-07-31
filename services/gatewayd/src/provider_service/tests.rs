use std::sync::Arc;

use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::{
    common::v1::{
        AppIdentity, ComponentKind, ComponentVersion, Destination, EvidenceLevel, FailureMode,
        IpFamily, Platform, RouteAction, TransportProtocol,
    },
    policy::v1::{Decision, DecisionSpec},
    provider::v1::{
        ConnectionContext, DecisionEvidence, DecisionRecord, ProviderKind, ProviderRequestContext,
        RegisterProviderRequest, ReportDecisionBatchRequest,
        provider_service_server::ProviderService,
    },
};
use nonproxy_storage::PolicyDatabase;
use tonic::Request;

use super::{ProviderRpcService, REQUIRED_CAPABILITIES, validate_batch_id, validate_registration};
use crate::{
    Gateway, credential_store::tests_support::MemoryCredentialStore,
    provider_session::PROVIDER_SESSION_LIFETIME_MS, session_capability::SessionCapability,
};

#[test]
fn registration_requires_matching_component_and_protocol() {
    let request = registration(ComponentKind::DnsProxy, 0, 0);

    assert!(validate_registration(&request).is_err());
    assert_eq!(PROVIDER_SESSION_LIFETIME_MS, 900_000);
}

#[test]
fn registration_rejects_protocol_range_without_server_minor() {
    let request = registration(ComponentKind::TransparentProxy, 2, 1);

    assert!(validate_registration(&request).is_err());
}

#[test]
fn registration_rejects_missing_or_duplicate_capabilities() {
    let mut request = registration(ComponentKind::TransparentProxy, 0, 0);
    request.capabilities = vec!["snapshot-v1".to_owned()];
    assert!(validate_registration(&request).is_err());

    request.capabilities = vec![
        "snapshot-v1".to_owned(),
        "heartbeat-v1".to_owned(),
        "heartbeat-v1".to_owned(),
    ];
    assert!(validate_registration(&request).is_err());
}

#[test]
fn report_batch_id_is_bounded_and_path_safe() {
    assert!(validate_batch_id("batch_1.retry-2").is_ok());
    assert!(validate_batch_id(&"a".repeat(128)).is_ok());
    assert!(validate_batch_id("").is_err());
    assert!(validate_batch_id(&"a".repeat(129)).is_err());
    assert!(validate_batch_id("../batch").is_err());
}

#[tokio::test]
async fn authenticated_report_is_persisted_and_replay_is_idempotent() {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("Provider 决策测试数据库打开失败: {error}"),
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    if let Err(error) = gateway.compile_and_stage().await {
        panic!("Provider 决策测试快照暂存失败: {error}");
    }
    let service = ProviderRpcService::with_credential_store(
        gateway.clone(),
        SessionCapability::from_token([2; 32]),
        Arc::new(MemoryCredentialStore::default()),
    );
    let registered = service
        .register_provider(Request::new(registration(
            ComponentKind::TransparentProxy,
            0,
            0,
        )))
        .await;
    let Ok(registered) = registered else {
        panic!("Provider 决策测试注册失败: {registered:?}");
    };
    let registered = registered.into_inner();
    let mut first_request = report_request(registered.session_token.clone(), 1);
    first_request.dropped_debug_events = 9;
    first_request.batch_id = "retry-batch".to_owned();
    let first = service
        .report_decision_batch(Request::new(first_request))
        .await;
    let mut replay_request = report_request(registered.session_token, 2);
    replay_request.dropped_debug_events = 9;
    replay_request.batch_id = "retry-batch".to_owned();
    let replay = service
        .report_decision_batch(Request::new(replay_request))
        .await;
    let listed = gateway.list_connection_decisions(10, 0).await;
    let status = gateway.status().await;

    assert!(matches!(first, Ok(response) if response.get_ref().accepted_count == 1));
    assert!(matches!(replay, Ok(response) if response.get_ref().accepted_count == 1));
    assert!(matches!(listed, Ok((records, 1)) if records.len() == 1));
    assert!(matches!(status, Ok(value) if value.dropped_decision_events == 9));
}

fn registration(
    component: ComponentKind,
    protocol_minor: u32,
    minimum_protocol_minor: u32,
) -> RegisterProviderRequest {
    RegisterProviderRequest {
        provider_instance_id: "transparent-1".to_owned(),
        kind: ProviderKind::TransparentProxy as i32,
        version: Some(ComponentVersion {
            component: component as i32,
            semantic_version: "1.0.0".to_owned(),
            build_id: "test".to_owned(),
            protocol_major: 1,
            protocol_minor,
            minimum_protocol_minor,
        }),
        capabilities: REQUIRED_CAPABILITIES
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        startup_nonce: vec![1; 32],
        bootstrap_capability: vec![2; 32],
    }
}

fn report_request(session_token: Vec<u8>, request_sequence: u64) -> ReportDecisionBatchRequest {
    ReportDecisionBatchRequest {
        context: Some(ProviderRequestContext {
            provider_instance_id: "transparent-1".to_owned(),
            session_token,
            request_sequence,
        }),
        decisions: vec![DecisionRecord {
            context: Some(ConnectionContext {
                flow_id: "provider-flow-1".to_owned(),
                app: Some(AppIdentity {
                    platform: Platform::Macos as i32,
                    stable_id: "com.example.browser".to_owned(),
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
                nanos: 10_000,
            }),
            error: None,
        }],
        dropped_debug_events: 0,
        batch_id: format!("batch-{request_sequence}"),
    }
}
