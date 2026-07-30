use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, Policy, PolicyId, PolicyMatch, PolicyMetadata,
    PolicyOrigin, PolicySourceKind,
};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::control::v1::{
    ApplyPolicySnapshotRequest, OperationContext, UpsertPolicyRequest,
    control_service_server::ControlService,
};
use nonproxy_storage::PolicyDatabase;
use tonic::{Code, Request};

use super::ControlRpcService;
use crate::{Gateway, proto_policy::policy_to_proto, session_capability::SessionCapability};

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
