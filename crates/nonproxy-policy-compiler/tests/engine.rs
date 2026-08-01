mod support;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use nonproxy_model::{
    AppIdentity, ConnectionContext, DecisionSpec, DomainMatchKind, FailureMode, OutboundId,
    Platform, Policy, PolicySourceKind, RouteAction, RuntimeOverrideMode, RuntimeRoutingOverride,
    Transport,
};
use nonproxy_policy::{
    CompiledPolicySnapshot, OutboundCapabilities, PolicyEngine, PolicyEvaluation,
};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use support::{
    app_identity, app_match, cidr_match, context, destination, domain_match, matcher, must_policy,
    port,
};

fn compile(policies: Vec<Policy>) -> CompiledPolicySnapshot {
    let result = PolicyCompiler::compile(CompileRequest::new(
        42,
        100,
        DecisionSpec::blocked(),
        policies,
        CompileCapabilities::full(),
    ));
    match result {
        Ok(value) => value,
        Err(error) => panic!("测试快照编译失败: {error:?}"),
    }
}

fn compile_with_override(
    policies: Vec<Policy>,
    runtime_override: RuntimeRoutingOverride,
) -> CompiledPolicySnapshot {
    PolicyCompiler::compile(
        CompileRequest::new(
            42,
            100,
            DecisionSpec::blocked(),
            policies,
            CompileCapabilities::full(),
        )
        .with_runtime_override(Some(runtime_override)),
    )
    .unwrap_or_else(|error| panic!("运行态覆盖测试快照编译失败: {error:?}"))
}

fn matched_id(snapshot: &CompiledPolicySnapshot, context: &ConnectionContext) -> Option<String> {
    PolicyEngine::decide(snapshot, context)
        .matched_policy_id()
        .map(ToString::to_string)
}

#[test]
fn fixed_tiers_prefer_system_then_app_destination_then_app() {
    let application = app_match("com.example.app");
    let domain = domain_match(DomainMatchKind::Suffix, "example.com");
    let system = must_policy(
        "system",
        PolicySourceKind::System,
        nonproxy_model::PolicyMatch::global(),
        DecisionSpec::blocked(),
        -100,
    );
    let app_destination = must_policy(
        "app-destination",
        PolicySourceKind::AppDestination,
        matcher(
            Some(application.clone()),
            Some(domain.clone()),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let app = must_policy(
        "app",
        PolicySourceKind::App,
        matcher(Some(application), None, None, None, Vec::new(), Vec::new()),
        DecisionSpec::direct(),
        100,
    );
    let request = context(
        "com.example.app",
        Some("api.example.com"),
        None,
        443,
        Transport::Tcp,
    );

    assert_eq!(
        matched_id(&compile(vec![app, app_destination, system]), &request),
        Some("system".to_owned())
    );
}

#[test]
fn system_rules_remain_authoritative_during_runtime_override() {
    let system = must_policy(
        "system",
        PolicySourceKind::System,
        nonproxy_model::PolicyMatch::global(),
        DecisionSpec::blocked(),
        -100,
    );
    let runtime_override = RuntimeRoutingOverride::new(RuntimeOverrideMode::Direct, None, 200)
        .unwrap_or_else(|error| panic!("运行态覆盖创建失败: {error}"));
    let snapshot = compile_with_override(vec![system], runtime_override);
    let request = context(
        "com.example.app",
        Some("example.com"),
        None,
        443,
        Transport::Tcp,
    );

    let PolicyEvaluation::Decision(decision) = PolicyEngine::evaluate_at(&snapshot, &request, 150)
    else {
        panic!("系统规则不应被暂停旁路");
    };
    assert_eq!(decision.result().action(), RouteAction::Block);
    assert_eq!(decision.reason_code(), "NP_POLICY_SYSTEM_MATCH");
}

#[test]
fn direct_override_precedes_user_rules_and_expires_without_snapshot_rebuild() {
    let user = must_policy(
        "blocked-site",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        100,
    );
    let runtime_override = RuntimeRoutingOverride::new(RuntimeOverrideMode::Direct, None, 200)
        .unwrap_or_else(|error| panic!("运行态覆盖创建失败: {error}"));
    let snapshot = compile_with_override(vec![user], runtime_override);
    let request = context(
        "com.example.app",
        Some("api.example.com"),
        None,
        443,
        Transport::Tcp,
    );

    let PolicyEvaluation::Decision(active) = PolicyEngine::evaluate_at(&snapshot, &request, 199)
    else {
        panic!("全部直连覆盖应产生决策");
    };
    let PolicyEvaluation::Decision(expired) = PolicyEngine::evaluate_at(&snapshot, &request, 200)
    else {
        panic!("覆盖到期后应恢复普通策略");
    };
    assert_eq!(active.result().action(), RouteAction::Direct);
    assert_eq!(active.reason_code(), "NP_RUNTIME_OVERRIDE_DIRECT");
    assert_eq!(expired.result().action(), RouteAction::Block);
    assert_eq!(
        expired.matched_policy_id().map(ToString::to_string),
        Some("blocked-site".to_owned())
    );
}

#[test]
fn pause_is_a_time_bounded_bypass_instead_of_direct() {
    let runtime_override = RuntimeRoutingOverride::new(RuntimeOverrideMode::Paused, None, 200)
        .unwrap_or_else(|error| panic!("运行态覆盖创建失败: {error}"));
    let snapshot = compile_with_override(Vec::new(), runtime_override);
    let request = context(
        "com.example.app",
        Some("example.com"),
        None,
        443,
        Transport::Tcp,
    );

    assert!(matches!(
        PolicyEngine::evaluate_at(&snapshot, &request, 199),
        PolicyEvaluation::Bypass {
            snapshot_version: 42,
            reason_code: "NP_RUNTIME_OVERRIDE_PAUSED"
        }
    ));
    let PolicyEvaluation::Decision(expired) = PolicyEngine::evaluate_at(&snapshot, &request, 200)
    else {
        panic!("暂停到期后应恢复普通策略");
    };
    assert_eq!(expired.result().action(), RouteAction::Block);
}

#[test]
fn proxy_override_is_fail_closed_and_uses_its_validated_outbound() {
    let outbound = OutboundId::new("emergency-proxy")
        .unwrap_or_else(|error| panic!("代理出口创建失败: {error}"));
    let runtime_override =
        RuntimeRoutingOverride::new(RuntimeOverrideMode::Proxy, Some(outbound.clone()), 200)
            .unwrap_or_else(|error| panic!("代理覆盖创建失败: {error}"));
    let snapshot = PolicyCompiler::compile(
        CompileRequest::new(
            42,
            100,
            DecisionSpec::blocked(),
            Vec::new(),
            CompileCapabilities::full()
                .with_outbound(outbound.clone(), OutboundCapabilities::full()),
        )
        .with_runtime_override(Some(runtime_override)),
    )
    .unwrap_or_else(|error| panic!("代理覆盖测试快照编译失败: {error:?}"));
    let request = context(
        "com.example.app",
        Some("example.com"),
        None,
        443,
        Transport::Tcp,
    );

    let PolicyEvaluation::Decision(decision) = PolicyEngine::evaluate_at(&snapshot, &request, 199)
    else {
        panic!("全部代理覆盖应产生代理决策");
    };
    assert_eq!(decision.result().action(), RouteAction::Proxy);
    assert_eq!(decision.result().outbound_id(), Some(&outbound));
    assert_eq!(decision.result().failure_mode(), FailureMode::Closed);
    assert_eq!(decision.reason_code(), "NP_RUNTIME_OVERRIDE_PROXY");
}

#[test]
fn app_destination_beats_app_and_app_beats_domain() {
    let app_destination = must_policy(
        "app-destination",
        PolicySourceKind::AppDestination,
        matcher(
            Some(app_match("com.example.app")),
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        -100,
    );
    let app = must_policy(
        "app",
        PolicySourceKind::App,
        matcher(
            Some(app_match("com.example.app")),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        -100,
    );
    let domain = must_policy(
        "domain",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        100,
    );
    let snapshot = compile(vec![domain, app, app_destination]);

    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "com.example.app",
                Some("api.example.com"),
                None,
                443,
                Transport::Tcp,
            ),
        ),
        Some("app-destination".to_owned())
    );
    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "com.example.app",
                Some("unrelated.test"),
                None,
                443,
                Transport::Tcp,
            ),
        ),
        Some("app".to_owned())
    );
}

#[test]
fn exact_domain_is_more_specific_than_registrable_and_suffix() {
    let suffix = must_policy(
        "suffix",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    );
    let registrable = must_policy(
        "registrable",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(
                DomainMatchKind::RegistrableDomain,
                "example.com",
            )),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    );
    let exact = must_policy(
        "exact",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Exact, "api.example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let snapshot = compile(vec![suffix, registrable, exact]);

    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "unknown-app",
                Some("api.example.com"),
                None,
                443,
                Transport::Tcp,
            ),
        ),
        Some("exact".to_owned())
    );
}

#[test]
fn cidr_radix_index_handles_ipv4_and_ipv6_longest_prefixes() {
    let ipv4_wide = must_policy(
        "ipv4-wide",
        PolicySourceKind::Cidr,
        matcher(
            None,
            None,
            Some(cidr_match("10.0.0.0/8")),
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    );
    let ipv4_narrow = must_policy(
        "ipv4-narrow",
        PolicySourceKind::Cidr,
        matcher(
            None,
            None,
            Some(cidr_match("10.10.0.0/16")),
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let ipv6 = must_policy(
        "ipv6",
        PolicySourceKind::Cidr,
        matcher(
            None,
            None,
            Some(cidr_match("2001:db8::/32")),
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let snapshot = compile(vec![ipv4_wide, ipv4_narrow, ipv6]);

    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "unknown-app",
                None,
                Some(IpAddr::V4(Ipv4Addr::new(10, 10, 1, 2))),
                443,
                Transport::Tcp,
            ),
        ),
        Some("ipv4-narrow".to_owned())
    );
    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "unknown-app",
                None,
                Some(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1,))),
                443,
                Transport::Tcp,
            ),
        ),
        Some("ipv6".to_owned())
    );
}

#[test]
fn transport_and_port_constraints_are_enforced() {
    let constrained = must_policy(
        "https-tcp",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            vec![Transport::Tcp],
            vec![port(443, 443)],
        ),
        DecisionSpec::direct(),
        10,
    );
    let fallback = must_policy(
        "site-fallback",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    );
    let snapshot = compile(vec![constrained, fallback]);

    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "unknown-app",
                Some("api.example.com"),
                None,
                443,
                Transport::Tcp,
            ),
        ),
        Some("https-tcp".to_owned())
    );
    assert_eq!(
        matched_id(
            &snapshot,
            &context(
                "unknown-app",
                Some("api.example.com"),
                None,
                443,
                Transport::Udp,
            ),
        ),
        Some("site-fallback".to_owned())
    );
}

#[test]
fn helper_matching_requires_opt_in_and_signer_match() {
    let signed_matcher = match app_match("com.example.parent").with_signer_id("TEAM1") {
        Ok(value) => value.include_helpers(true),
        Err(error) => panic!("测试签名匹配器创建失败: {error}"),
    };
    let policy = must_policy(
        "signed-helper",
        PolicySourceKind::App,
        matcher(
            Some(signed_matcher),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let snapshot = compile(vec![policy]);
    let helper = match app_identity("com.example.helper")
        .with_parent_stable_id("com.example.parent")
        .and_then(|value| value.with_signer_id("TEAM1"))
    {
        Ok(value) => value,
        Err(error) => panic!("测试 helper 身份创建失败: {error}"),
    };
    let wrong_signer = match app_identity("com.example.helper")
        .with_parent_stable_id("com.example.parent")
        .and_then(|value| value.with_signer_id("TEAM2"))
    {
        Ok(value) => value,
        Err(error) => panic!("测试错误签名身份创建失败: {error}"),
    };
    let target = destination(Some("example.com"), None, 443, Transport::Tcp);

    assert_eq!(
        matched_id(&snapshot, &ConnectionContext::new(helper, target.clone()),),
        Some("signed-helper".to_owned())
    );
    assert_eq!(
        matched_id(&snapshot, &ConnectionContext::new(wrong_signer, target),),
        None
    );
}

#[test]
fn unknown_app_does_not_implicitly_select_an_app_rule() {
    let app = must_policy(
        "known-app",
        PolicySourceKind::App,
        matcher(
            Some(app_match("com.example.app")),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let domain = must_policy(
        "domain",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let snapshot = compile(vec![app, domain]);
    let request = ConnectionContext::new(
        AppIdentity::unknown(Platform::MacOs),
        destination(Some("api.example.com"), None, 443, Transport::Tcp),
    );

    assert_eq!(matched_id(&snapshot, &request), Some("domain".to_owned()));
}

#[test]
fn unknown_app_can_only_match_an_explicit_unknown_identity_rule() {
    let explicit_unknown = must_policy(
        "explicit-unknown",
        PolicySourceKind::App,
        matcher(
            Some(app_match("unknown-app")),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let snapshot = compile(vec![explicit_unknown]);
    let request = ConnectionContext::new(
        AppIdentity::unknown(Platform::MacOs),
        destination(Some("example.com"), None, 443, Transport::Tcp),
    );

    assert_eq!(
        matched_id(&snapshot, &request),
        Some("explicit-unknown".to_owned())
    );
}
