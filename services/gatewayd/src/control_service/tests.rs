use std::sync::Arc;

use nonproxy_model::{
    DecisionSpec, DomainMatchKind, DomainMatcher, FailureMode, OutboundId, Policy, PolicyId,
    PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind, RouteAction,
};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::control::v1::{
    ApplyPolicySnapshotRequest, CapabilityName, ConfirmLearningCandidatesRequest, DefaultRouteKind,
    DiagnosticRedactionLevel, ExitProbeRouteKind, ExportDiagnosticsRequest, GetCapabilitiesRequest,
    GetSystemStatusRequest, ImportConfigurationRequest, LearningObservationKind,
    LearningResourceType, LearningSessionKind, ListExitProbesRequest,
    ListLearningCandidatesRequest, ListOutboundsRequest, OperationContext,
    RecordLearningObservationRequest, SetDefaultRouteRequest, StartLearningSessionRequest,
    StopLearningSessionRequest, TestOutboundRequest, UpsertPolicyRequest, VerifyExitRequest,
    control_service_server::ControlService, set_default_route_request,
    start_learning_session_request,
};
use nonproxy_proto::events::v1::{LearningCandidateKind, RuntimeState, event_envelope};
use nonproxy_storage::{ExitProbeRoute, OutboundKind, OutboundReference, PolicyDatabase};
use tonic::{Code, Request};

use crate::control_rpc_service::ControlRpcService;
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
async fn low_privilege_mutation_rejects_protected_source_and_reserved_id() {
    let service = service([7; 32]);
    let protected = protected_policy("attacker-system");
    let reserved = site_policy("system-macos-gateway-direct", "example.com");

    for (index, policy) in [protected, reserved].into_iter().enumerate() {
        let response = service
            .upsert_policy(Request::new(UpsertPolicyRequest {
                context: Some(context([7; 32], &format!("save-protected-{index}"))),
                policy: Some(policy_to_proto(&policy)),
                expected_revision: 0,
            }))
            .await;
        let Ok(response) = response else {
            panic!("受保护策略拒绝响应失败: {response:?}");
        };
        assert!(matches!(
            response.into_inner().result.and_then(|value| value.error),
            Some(error) if error.code == "NP_REQUEST_INVALID"
        ));
    }
}

#[tokio::test]
async fn test_outbound_requires_the_exact_session_capability() {
    let service = service([7; 32]);
    let request = TestOutboundRequest {
        context: Some(context([8; 32], "test-outbound")),
        outbound_id: "primary".to_owned(),
        timeout: Some(prost_types::Duration {
            seconds: 2,
            nanos: 0,
        }),
    };

    let result = service.test_outbound(Request::new(request)).await;

    assert!(matches!(
        result,
        Err(status) if status.code() == Code::PermissionDenied
    ));
}

#[tokio::test]
async fn verify_exit_requires_the_exact_session_capability() {
    let service = service([7; 32]);
    let request = VerifyExitRequest {
        context: Some(context([8; 32], "verify-exit")),
        route: ExitProbeRouteKind::Direct as i32,
        outbound_id: String::new(),
        timeout: Some(prost_types::Duration {
            seconds: 2,
            nanos: 0,
        }),
    };

    let result = service.verify_exit(Request::new(request)).await;

    assert!(matches!(
        result,
        Err(status) if status.code() == Code::PermissionDenied
    ));
}

#[tokio::test]
async fn export_diagnostics_requires_the_exact_session_capability() {
    let service = service([7; 32]);
    let result = service
        .export_diagnostics(Request::new(ExportDiagnosticsRequest {
            context: Some(context([8; 32], "export-diagnostics")),
            redaction_level: DiagnosticRedactionLevel::Strict as i32,
            time_range: None,
        }))
        .await;

    assert!(matches!(
        result,
        Err(status) if status.code() == Code::PermissionDenied
    ));
}

#[tokio::test]
async fn exit_probe_capability_is_only_advertised_when_trust_is_configured() {
    let without_probe = service([7; 32]);
    let without = without_probe
        .get_capabilities(Request::new(GetCapabilitiesRequest {}))
        .await
        .unwrap_or_else(|error| panic!("未配置能力读取失败: {error}"))
        .into_inner();
    assert!(
        !without
            .capabilities
            .contains(&(CapabilityName::ExitProbe as i32))
    );

    let signer = nonproxy_exit_probe::ExitProbeSigner::from_secret_bytes(&[7; 32])
        .unwrap_or_else(|error| panic!("测试探针签名器创建失败: {error}"));
    let verifier =
        nonproxy_exit_probe::ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64())
            .unwrap_or_else(|error| panic!("测试探针验证器创建失败: {error}"));
    let endpoint = nonproxy_exit_probe::ExitProbeEndpoint::parse("https://probe.example/v1/exit")
        .unwrap_or_else(|error| panic!("测试探针地址无效: {error}"));
    let client = nonproxy_exit_probe::ExitProbeClient::new(endpoint, verifier)
        .unwrap_or_else(|error| panic!("测试探针客户端创建失败: {error}"));
    let with_probe = service([7; 32]).with_exit_probe_client(Some(client));
    let with = with_probe
        .get_capabilities(Request::new(GetCapabilitiesRequest {}))
        .await
        .unwrap_or_else(|error| panic!("已配置能力读取失败: {error}"))
        .into_inner();

    assert!(
        with.capabilities
            .contains(&(CapabilityName::ExitProbe as i32))
    );
}

#[tokio::test]
async fn list_exit_probes_returns_persisted_receipt_and_configuration_state() {
    let service = service([7; 32]);
    let verified = verified_exit_fixture();
    service
        .gateway
        .save_exit_probe(ExitProbeRoute::Direct, &verified)
        .await
        .unwrap_or_else(|error| panic!("测试出口回执保存失败: {error}"));
    let without = service
        .list_exit_probes(Request::new(ListExitProbesRequest { page: None }))
        .await
        .unwrap_or_else(|error| panic!("未配置出口回执查询失败: {error}"))
        .into_inner();

    assert!(!without.verification_available);
    assert_eq!(without.total_count, 1);
    let receipt = without
        .probes
        .first()
        .unwrap_or_else(|| panic!("缺少已持久化的出口回执"));
    assert_eq!(receipt.observed_ip, "8.8.8.8");
    assert_eq!(receipt.route(), ExitProbeRouteKind::Direct);
    assert!(receipt.observed_at.is_some());
    assert!(receipt.verified_at.is_some());

    let signer = nonproxy_exit_probe::ExitProbeSigner::from_secret_bytes(&[7; 32])
        .unwrap_or_else(|error| panic!("测试探针签名器创建失败: {error}"));
    let verifier =
        nonproxy_exit_probe::ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64())
            .unwrap_or_else(|error| panic!("测试探针验证器创建失败: {error}"));
    let endpoint = nonproxy_exit_probe::ExitProbeEndpoint::parse("https://probe.example/v1/exit")
        .unwrap_or_else(|error| panic!("测试探针地址无效: {error}"));
    let client = nonproxy_exit_probe::ExitProbeClient::new(endpoint, verifier)
        .unwrap_or_else(|error| panic!("测试探针客户端创建失败: {error}"));
    let with = service
        .with_exit_probe_client(Some(client))
        .list_exit_probes(Request::new(ListExitProbesRequest { page: None }))
        .await
        .unwrap_or_else(|error| panic!("已配置出口回执查询失败: {error}"))
        .into_inner();

    assert!(with.verification_available);
    assert_eq!(with.total_count, 1);
}

#[tokio::test]
async fn list_outbounds_returns_fresh_probe_observation() {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("出口健康列表测试数据库打开失败: {error}"),
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let outbound_id = match OutboundId::new("primary") {
        Ok(value) => value,
        Err(error) => panic!("出口健康列表测试 ID 创建失败: {error}"),
    };
    let outbound = match OutboundReference::new(
        outbound_id.clone(),
        OutboundKind::HttpConnect,
        Some("127.0.0.1"),
        Some(8_080),
        None,
        1,
    ) {
        Ok(value) => value,
        Err(error) => panic!("出口健康列表测试配置创建失败: {error}"),
    };
    if let Err(error) = gateway.save_outbounds(vec![(outbound, None)]).await {
        panic!("出口健康列表测试配置保存失败: {error}");
    }
    let now = match crate::clock::unix_time_ms() {
        Ok(value) => value,
        Err(error) => panic!("出口健康列表测试时间读取失败: {error}"),
    };
    if let Err(error) =
        gateway.report_outbound_health(outbound_id, 1, RuntimeState::Ready, Some(42), now)
    {
        panic!("出口健康列表测试状态写入失败: {error}");
    }
    let service = ControlRpcService::new(gateway, SessionCapability::from_token([7; 32]));

    let response = service
        .list_outbounds(Request::new(ListOutboundsRequest { page: None }))
        .await;
    let Ok(response) = response else {
        panic!("出口健康列表 RPC 失败: {response:?}");
    };
    let outbound = match response.into_inner().outbounds.into_iter().next() {
        Some(value) => value,
        None => panic!("出口健康列表必须返回测试配置"),
    };

    assert_eq!(outbound.health, RuntimeState::Ready as i32);
    assert!(outbound.last_checked_at.is_some());
    assert!(matches!(
        outbound.latency,
        Some(value) if value.seconds == 0 && value.nanos == 42_000_000
    ));
}

#[tokio::test]
async fn authenticated_default_route_change_is_visible_in_status_and_catalog() {
    let database = match PolicyDatabase::open_in_memory(1) {
        Ok(value) => value,
        Err(error) => panic!("默认路由 RPC 测试数据库打开失败: {error}"),
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let outbound_id = match OutboundId::new("primary") {
        Ok(value) => value,
        Err(error) => panic!("默认路由 RPC 测试 ID 创建失败: {error}"),
    };
    let outbound = match OutboundReference::new(
        outbound_id,
        OutboundKind::Socks5,
        Some("127.0.0.1"),
        Some(1_080),
        None,
        1,
    ) {
        Ok(value) => value,
        Err(error) => panic!("默认路由 RPC 测试出口创建失败: {error}"),
    };
    if let Err(error) = gateway.save_outbounds(vec![(outbound, None)]).await {
        panic!("默认路由 RPC 测试出口保存失败: {error}");
    }
    let service = ControlRpcService::new(gateway.clone(), SessionCapability::from_token([7; 32]));

    let changed = service
        .set_default_route(Request::new(SetDefaultRouteRequest {
            context: Some(context([7; 32], "set-default-route")),
            expected_routing_revision: 1,
            route: Some(set_default_route_request::Route::OutboundId(
                "primary".to_owned(),
            )),
        }))
        .await;
    let Ok(changed) = changed else {
        panic!("默认路由 RPC 修改失败: {changed:?}");
    };
    let changed = changed.into_inner();
    assert!(changed.error.is_none());
    assert_eq!(changed.routing_revision, 2);
    assert!(matches!(
        changed.snapshot,
        Some(value) if value.snapshot_version == 1
    ));

    let status = service
        .get_system_status(Request::new(GetSystemStatusRequest {}))
        .await;
    let Ok(status) = status else {
        panic!("默认路由状态读取失败: {status:?}");
    };
    let status = status.into_inner();
    assert_eq!(status.default_route, DefaultRouteKind::Proxy as i32);
    assert_eq!(status.default_outbound_id, "primary");
    assert_eq!(status.routing_revision, 2);

    let listed = service
        .list_outbounds(Request::new(ListOutboundsRequest { page: None }))
        .await;
    let Ok(listed) = listed else {
        panic!("默认出口列表读取失败: {listed:?}");
    };
    let listed = listed.into_inner();
    assert_eq!(listed.routing_revision, 2);
    assert!(matches!(
        listed.outbounds.as_slice(),
        [value] if value.id == "primary" && value.is_default
    ));

    let stale = service
        .set_default_route(Request::new(SetDefaultRouteRequest {
            context: Some(context([7; 32], "set-default-route-stale")),
            expected_routing_revision: 1,
            route: Some(set_default_route_request::Route::Direct(true)),
        }))
        .await;
    assert!(matches!(
        stale,
        Ok(value)
            if value.get_ref().routing_revision == 0
                && matches!(
                    value.get_ref().error.as_ref(),
                    Some(error) if error.code == "NP_ROUTING_REVISION_CONFLICT"
                )
    ));
}

#[tokio::test]
async fn default_route_change_requires_the_exact_session_capability() {
    let service = service([7; 32]);
    let result = service
        .set_default_route(Request::new(SetDefaultRouteRequest {
            context: Some(context([8; 32], "set-default-route")),
            expected_routing_revision: 1,
            route: Some(set_default_route_request::Route::Direct(true)),
        }))
        .await;

    assert!(matches!(
        result,
        Err(status) if status.code() == Code::PermissionDenied
    ));
}

#[tokio::test]
async fn authenticated_request_can_restore_direct_as_an_explicit_route() {
    let service = service([7; 32]);
    let result = service
        .set_default_route(Request::new(SetDefaultRouteRequest {
            context: Some(context([7; 32], "set-direct-route")),
            expected_routing_revision: 1,
            route: Some(set_default_route_request::Route::Direct(true)),
        }))
        .await;
    let Ok(result) = result else {
        panic!("恢复默认直连 RPC 失败: {result:?}");
    };
    let result = result.into_inner();

    assert!(result.error.is_none());
    assert_eq!(result.routing_revision, 2);
    assert!(matches!(
        result.snapshot,
        Some(value) if value.snapshot_version == 1
    ));
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

#[tokio::test]
async fn site_learning_rpc_is_bounded_tab_scoped_and_evented() {
    let database = PolicyDatabase::open_in_memory(1);
    let Ok(database) = database else {
        panic!("学习 RPC 测试数据库打开失败: {database:?}");
    };
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let service = ControlRpcService::new(gateway.clone(), SessionCapability::from_token([7; 32]));
    let started = service
        .start_learning_session(Request::new(StartLearningSessionRequest {
            context: Some(context([7; 32], "start-learning")),
            kind: LearningSessionKind::Site as i32,
            duration: Some(prost_types::Duration {
                seconds: 60,
                nanos: 0,
            }),
            browser_context_id: "browser-context-a".to_owned(),
            subject: Some(start_learning_session_request::Subject::NormalizedSite(
                "example.com".to_owned(),
            )),
        }))
        .await;
    let Ok(started) = started else {
        panic!("学习会话启动 RPC 失败: {started:?}");
    };
    let started = started.into_inner();
    assert!(started.error.is_none());
    assert!(!started.session_id.is_empty());

    let record = learning_observation(&started.session_id, "browser-context-a");
    let recorded = service
        .record_learning_observation(Request::new(record.clone()))
        .await;
    let Ok(recorded) = recorded else {
        panic!("学习观测 RPC 失败: {recorded:?}");
    };
    let recorded = recorded.into_inner();
    let Some(candidate) = recorded.candidate else {
        panic!("学习观测必须返回聚合候选");
    };
    assert!(!recorded.duplicate);
    assert_eq!(candidate.normalized_domain, "api.example.com");
    assert_eq!(
        candidate.kind,
        LearningCandidateKind::RequiredFirstParty as i32
    );
    assert!(candidate.requires_confirmation);

    let replayed = service
        .record_learning_observation(Request::new(record))
        .await;
    assert!(matches!(
        replayed,
        Ok(value) if value.get_ref().duplicate
    ));
    let events = gateway.events().subscribe(0);
    let Ok((events, _receiver)) = events else {
        panic!("学习候选事件读取失败: {events:?}");
    };
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].payload.as_ref(),
        Some(event_envelope::Payload::LearningCandidateUpdated(value))
            if value.session_id == started.session_id
    ));

    let listed = service
        .list_learning_candidates(Request::new(ListLearningCandidatesRequest {
            context: Some(context([7; 32], "list-learning")),
            session_id: started.session_id.clone(),
        }))
        .await;
    assert!(matches!(
        listed,
        Ok(value) if value.get_ref().candidates.len() == 1
    ));

    let stopped = service
        .stop_learning_session(Request::new(StopLearningSessionRequest {
            context: Some(context([7; 32], "stop-learning")),
            session_id: started.session_id,
        }))
        .await;
    assert!(matches!(
        stopped,
        Ok(value) if value.get_ref().candidate_count == 1
    ));
}

#[tokio::test]
async fn learning_rpc_rejects_cross_tab_and_non_normalized_domains() {
    let service = service([7; 32]);
    let mut start = StartLearningSessionRequest {
        context: Some(context([7; 32], "start-learning")),
        kind: LearningSessionKind::Site as i32,
        duration: None,
        browser_context_id: "browser-context-a".to_owned(),
        subject: Some(start_learning_session_request::Subject::NormalizedSite(
            "Example.COM".to_owned(),
        )),
    };
    let invalid = service
        .start_learning_session(Request::new(start.clone()))
        .await;
    assert!(matches!(invalid, Err(status) if status.code() == Code::InvalidArgument));

    start.subject = Some(start_learning_session_request::Subject::NormalizedSite(
        "example.com".to_owned(),
    ));
    let started = service.start_learning_session(Request::new(start)).await;
    let Ok(started) = started else {
        panic!("跨标签页测试会话启动失败: {started:?}");
    };
    let session_id = started.into_inner().session_id;
    let result = service
        .record_learning_observation(Request::new(learning_observation(
            &session_id,
            "browser-context-b",
        )))
        .await;
    let Ok(result) = result else {
        panic!("跨标签页拒绝应返回结构化错误: {result:?}");
    };
    assert!(matches!(
        result.into_inner().error,
        Some(error) if error.code == "NP_LEARNING_BROWSER_CONTEXT_MISMATCH"
    ));
}

#[tokio::test]
async fn candidate_confirmation_rpc_is_authenticated_and_idempotent() {
    let service = service([7; 32]);
    let started = start_site_learning(&service).await;
    for request in [
        learning_observation_for(
            &started,
            "observation-main",
            "example.com",
            LearningObservationKind::MainFrame,
        ),
        learning_observation_for(
            &started,
            "observation-api",
            "api.example.com",
            LearningObservationKind::Subresource,
        ),
    ] {
        if let Err(error) = service
            .record_learning_observation(Request::new(request))
            .await
        {
            panic!("确认测试学习观测失败: {error}");
        }
    }
    if let Err(error) = service
        .stop_learning_session(Request::new(StopLearningSessionRequest {
            context: Some(context([7; 32], "stop-confirm-learning")),
            session_id: started.clone(),
        }))
        .await
    {
        panic!("确认测试停止学习失败: {error}");
    }
    let request = ConfirmLearningCandidatesRequest {
        context: Some(context([7; 32], "confirm-learning")),
        session_id: started,
        confirmation_id: "confirmation-a".to_owned(),
        selected_domains: vec!["example.com".to_owned(), "api.example.com".to_owned()],
    };

    let first = service
        .confirm_learning_candidates(Request::new(request.clone()))
        .await;
    let Ok(first) = first else {
        panic!("候选确认 RPC 失败: {first:?}");
    };
    let first = first.into_inner();
    assert!(first.error.is_none());
    assert!(!first.replayed);
    assert_eq!(first.policies.len(), 2);
    assert!(matches!(
        first.snapshot,
        Some(value) if value.snapshot_version == 1
    ));

    let replay = service
        .confirm_learning_candidates(Request::new(request))
        .await;
    assert!(matches!(
        replay,
        Ok(value) if value.get_ref().replayed
            && value.get_ref().policies == first.policies
    ));
}

async fn start_site_learning(service: &ControlRpcService) -> String {
    let started = service
        .start_learning_session(Request::new(StartLearningSessionRequest {
            context: Some(context([7; 32], "start-confirm-learning")),
            kind: LearningSessionKind::Site as i32,
            duration: None,
            browser_context_id: "browser-context-a".to_owned(),
            subject: Some(start_learning_session_request::Subject::NormalizedSite(
                "example.com".to_owned(),
            )),
        }))
        .await;
    let Ok(started) = started else {
        panic!("确认测试学习会话启动失败: {started:?}");
    };
    started.into_inner().session_id
}

fn learning_observation_for(
    session_id: &str,
    observation_id: &str,
    domain: &str,
    kind: LearningObservationKind,
) -> RecordLearningObservationRequest {
    RecordLearningObservationRequest {
        context: Some(context([7; 32], observation_id)),
        session_id: session_id.to_owned(),
        observation_id: observation_id.to_owned(),
        browser_context_id: "browser-context-a".to_owned(),
        kind: kind as i32,
        normalized_domain: domain.to_owned(),
        initiator_domain: "example.com".to_owned(),
        resource_type: LearningResourceType::Fetch as i32,
    }
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

fn verified_exit_fixture() -> nonproxy_exit_probe::VerifiedExitProbe {
    let signer = nonproxy_exit_probe::ExitProbeSigner::from_secret_bytes(&[9; 32])
        .unwrap_or_else(|error| panic!("测试出口签名器创建失败: {error}"));
    let verifier =
        nonproxy_exit_probe::ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64())
            .unwrap_or_else(|error| panic!("测试出口验证器创建失败: {error}"));
    let nonce =
        nonproxy_exit_probe::ProbeNonce::from_base64("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            .unwrap_or_else(|error| panic!("测试出口 nonce 无效: {error}"));
    let observed_at = crate::clock::unix_time_ms()
        .unwrap_or_else(|error| panic!("测试出口时间读取失败: {error}"));
    let receipt = signer
        .sign(
            nonce,
            std::net::Ipv4Addr::new(8, 8, 8, 8).into(),
            observed_at,
        )
        .unwrap_or_else(|error| panic!("测试出口回执签名失败: {error}"));
    verifier
        .verify(nonce, receipt, observed_at)
        .unwrap_or_else(|error| panic!("测试出口回执验证失败: {error}"))
}

fn learning_observation(
    session_id: &str,
    browser_context_id: &str,
) -> RecordLearningObservationRequest {
    RecordLearningObservationRequest {
        context: Some(context([7; 32], "record-learning")),
        session_id: session_id.to_owned(),
        observation_id: "observation-a".to_owned(),
        browser_context_id: browser_context_id.to_owned(),
        kind: LearningObservationKind::Subresource as i32,
        normalized_domain: "api.example.com".to_owned(),
        initiator_domain: "example.com".to_owned(),
        resource_type: LearningResourceType::Fetch as i32,
    }
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

fn protected_policy(id: &str) -> Policy {
    let id = match PolicyId::new(id) {
        Ok(value) => value,
        Err(error) => panic!("受保护测试策略 ID 创建失败: {error}"),
    };
    match Policy::new(
        id,
        "伪造系统策略",
        PolicyMatch::global(),
        DecisionSpec::direct(),
        PolicyMetadata::new(PolicySourceKind::System, i32::MAX, PolicyOrigin::System, 1),
    ) {
        Ok(value) => value,
        Err(error) => panic!("受保护测试策略创建失败: {error}"),
    }
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
