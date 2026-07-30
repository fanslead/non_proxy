use std::sync::Arc;

use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, FailureMode, OutboundId, Policy, PolicyId,
    PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind, RouteAction,
};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::control::v1::{
    ApplyPolicySnapshotRequest, ImportConfigurationRequest, OperationContext, UpsertPolicyRequest,
    control_service_server::ControlService,
};
use nonproxy_storage::PolicyDatabase;
use tonic::{Code, Request};

use super::ControlRpcService;
use crate::{
    Gateway, credential_store::tests_support::MemoryCredentialStore, proto_policy::policy_to_proto,
    session_capability::SessionCapability,
};

#[tokio::test]
async fn mutation_requires_the_exact_session_capability() {
    let service = service([7; 32]);
    let request = UpsertPolicyRequest {
        context: Some(context([8; 32], "save-policy")),
        policy: Some(policy_to_proto(&site_policy("policy-a", "example.com"))),
        expected_revision: 0,
    };

    let result = service.upsert_policy(Request::new(request)).await;

    let Err(status) = result else {
        panic!("错误令牌必须被拒绝");
    };
    assert_eq!(status.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn authenticated_policy_can_be_saved_then_staged() {
    let service = service([7; 32]);
    let save = UpsertPolicyRequest {
        context: Some(context([7; 32], "save-policy")),
        policy: Some(policy_to_proto(&site_policy("policy-a", "example.com"))),
        expected_revision: 0,
    };
    let saved = service.upsert_policy(Request::new(save)).await;
    let Ok(saved) = saved else {
        panic!("策略保存 RPC 失败: {saved:?}");
    };
    let saved = saved.into_inner().result;
    assert!(
        saved
            .as_ref()
            .and_then(|value| value.error.as_ref())
            .is_none()
    );

    let apply = ApplyPolicySnapshotRequest {
        context: Some(context([7; 32], "apply-policy")),
    };
    let applied = service.apply_policy_snapshot(Request::new(apply)).await;
    let Ok(applied) = applied else {
        panic!("策略发布 RPC 失败: {applied:?}");
    };
    let snapshot = applied
        .into_inner()
        .result
        .and_then(|result| result.snapshot);
    let Some(snapshot) = snapshot else {
        panic!("策略发布必须返回快照");
    };
    assert_eq!(snapshot.snapshot_version, 1);
    assert_eq!(
        snapshot.state,
        nonproxy_proto::policy::v1::SnapshotState::PendingAck as i32
    );
}

#[tokio::test]
async fn authenticated_import_stores_secret_outside_database() {
    let database = PolicyDatabase::open_in_memory(1);
    let Ok(database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let credentials = Arc::new(MemoryCredentialStore::default());
    let service = ControlRpcService::with_credential_store(
        gateway.clone(),
        SessionCapability::from_token([7; 32]),
        credentials.clone(),
    );
    let request = import_request(false);

    let response = service.import_configuration(Request::new(request)).await;
    let Ok(response) = response else {
        panic!("出口导入 RPC 失败: {response:?}");
    };
    let response = response.into_inner();

    assert!(response.error.is_none());
    assert_eq!(response.outbounds.len(), 1);
    assert_eq!(response.outbounds[0].endpoint_host, "127.0.0.1");
    let stored = gateway.list_outbounds().await;
    let Ok(stored) = stored else {
        panic!("读取导入出口失败: {stored:?}");
    };
    let Some(reference) = stored[0]
        .credential()
        .map(nonproxy_storage::CredentialReference::item_reference)
    else {
        panic!("导入出口必须只保存凭据引用");
    };
    assert!(credentials.contains(reference));

    let saved = gateway
        .save_policy(proxy_site_policy("primary"), None)
        .await;
    let Ok(saved) = saved else {
        panic!("保存代理策略失败: {saved:?}");
    };
    assert_eq!(saved.decision().action(), RouteAction::Proxy);
    let compiled = gateway.compile_and_stage().await;
    let Ok(compiled) = compiled else {
        panic!("导入的 SOCKS5 出口必须参与策略编译: {compiled:?}");
    };
    let decoded = crate::snapshot_payload::decode(compiled.artifact().payload());
    let Ok((_, capabilities, _)) = decoded else {
        panic!("读取已编译代理快照失败: {decoded:?}");
    };
    let Some(outbound) = saved.decision().outbound_id() else {
        panic!("代理决策应包含出口");
    };
    assert!(capabilities.outbounds().contains_key(outbound));
}

#[tokio::test]
async fn validate_only_import_does_not_write_metadata_or_credentials() {
    let database = PolicyDatabase::open_in_memory(1);
    let Ok(database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let credentials = Arc::new(MemoryCredentialStore::default());
    let service = ControlRpcService::with_credential_store(
        gateway.clone(),
        SessionCapability::from_token([7; 32]),
        credentials.clone(),
    );

    let response = service
        .import_configuration(Request::new(import_request(true)))
        .await;
    let Ok(response) = response else {
        panic!("出口校验 RPC 失败: {response:?}");
    };

    assert!(response.into_inner().error.is_none());
    let stored = gateway.list_outbounds().await;
    assert!(matches!(stored, Ok(values) if values.is_empty()));
    assert!(credentials.is_empty());
}

fn service(token: [u8; 32]) -> ControlRpcService {
    let database = PolicyDatabase::open_in_memory(1);
    let Ok(database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    ControlRpcService::new(
        Gateway::new(database, CompileCapabilities::full()),
        SessionCapability::from_token(token),
    )
}

fn context(token: [u8; 32], operation_id: &str) -> OperationContext {
    OperationContext {
        operation_id: operation_id.to_owned(),
        session_capability_token: token.to_vec(),
    }
}

fn import_request(validate_only: bool) -> ImportConfigurationRequest {
    ImportConfigurationRequest {
        context: Some(context([7; 32], "import-outbound")),
        format: "nonproxy-json-v1".to_owned(),
        configuration: br#"{
            "version": 1,
            "outbounds": [{
                "id": "primary",
                "kind": "socks5",
                "host": "127.0.0.1",
                "port": 1080,
                "username": "alice",
                "password": "private"
            }]
        }"#
        .to_vec(),
        validate_only,
    }
}

fn site_policy(id: &str, domain: &str) -> Policy {
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
        PolicyMetadata::new(PolicySourceKind::Site, 100, PolicyOrigin::User, 1),
    );
    let Ok(policy) = policy else {
        panic!("测试策略创建失败: {policy:?}");
    };
    policy
}

fn proxy_site_policy(outbound: &str) -> Policy {
    let matcher = DomainMatcher::new(DomainMatchKind::Exact, "proxy.example").and_then(|domain| {
        PolicyMatch::new(None, Some(domain), None, None, Vec::new(), Vec::new())
    });
    let Ok(matcher) = matcher else {
        panic!("代理测试域名匹配器创建失败: {matcher:?}");
    };
    let id = PolicyId::new("proxy-policy");
    let outbound = OutboundId::new(outbound);
    let (Ok(id), Ok(outbound)) = (id, outbound) else {
        panic!("代理测试标识创建失败");
    };
    let decision = DecisionSpec::new(RouteAction::Proxy, Some(outbound), FailureMode::Closed);
    let Ok(decision) = decision else {
        panic!("代理测试决策创建失败: {decision:?}");
    };
    let policy = Policy::new(
        id,
        "代理网站",
        matcher,
        decision,
        PolicyMetadata::new(PolicySourceKind::Site, 100, PolicyOrigin::User, 1),
    );
    let Ok(policy) = policy else {
        panic!("代理测试策略创建失败: {policy:?}");
    };
    policy
}
