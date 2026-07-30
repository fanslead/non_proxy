mod support;

use nonproxy_model::{
    ConnectionContext, DecisionSpec, DomainMatchKind, NetworkProfileId, Policy, PolicySourceKind,
    Transport,
};
use nonproxy_policy::{CompiledPolicySnapshot, PolicyEngine};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use support::{app_match, context, domain_match, matcher, must_policy, network_match};

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

fn matched_id(snapshot: &CompiledPolicySnapshot, context: &ConnectionContext) -> Option<String> {
    PolicyEngine::decide(snapshot, context)
        .matched_policy_id()
        .map(ToString::to_string)
}

#[test]
fn network_then_builtin_then_default_complete_the_fallback_chain() {
    let network = must_policy(
        "office",
        PolicySourceKind::Network,
        matcher(
            None,
            None,
            None,
            Some(network_match("office")),
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let built_in = must_policy(
        "builtin",
        PolicySourceKind::BuiltIn,
        nonproxy_model::PolicyMatch::global(),
        DecisionSpec::direct(),
        0,
    );
    let network_id = match NetworkProfileId::new("office") {
        Ok(value) => value,
        Err(error) => panic!("测试网络标识创建失败: {error}"),
    };
    let request = context(
        "unknown-app",
        Some("example.com"),
        None,
        443,
        Transport::Tcp,
    )
    .with_network_profile(network_id);

    assert_eq!(
        matched_id(&compile(vec![built_in.clone(), network]), &request),
        Some("office".to_owned())
    );
    assert_eq!(
        matched_id(&compile(vec![built_in]), &request),
        Some("builtin".to_owned())
    );
    let decision = PolicyEngine::decide(&compile(Vec::new()), &request);
    assert_eq!(decision.reason_code(), "NP_POLICY_DEFAULT");
    assert!(decision.matched_policy_id().is_none());
    assert_eq!(decision.snapshot_version(), 42);
}

#[test]
fn adapter_rule_uses_its_actual_match_dimensions_for_tier() {
    let adapter = must_policy(
        "adapter-app-destination",
        PolicySourceKind::Adapter,
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
        100,
    );
    let snapshot = compile(vec![app, adapter]);

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
        Some("adapter-app-destination".to_owned())
    );
}

#[test]
fn app_destination_uses_destination_specificity_inside_app_bucket() {
    let suffix = must_policy(
        "app-suffix",
        PolicySourceKind::AppDestination,
        matcher(
            Some(app_match("com.example.app")),
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    );
    let exact = must_policy(
        "app-exact",
        PolicySourceKind::AppDestination,
        matcher(
            Some(app_match("com.example.app")),
            Some(domain_match(DomainMatchKind::Exact, "api.example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let snapshot = compile(vec![suffix, exact]);

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
        Some("app-exact".to_owned())
    );
}
