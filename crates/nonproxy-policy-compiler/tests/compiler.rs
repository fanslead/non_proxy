mod support;

use nonproxy_model::{
    DecisionSpec, DomainMatchKind, FailureMode, NetworkFingerprint, NetworkFingerprintKind,
    NetworkMatcher, NetworkProfileBinding, NetworkProfileId, OutboundId, PolicySourceKind,
    RouteAction,
};
use nonproxy_policy_compiler::{
    CompileCapabilities, CompileError, CompileRequest, OutboundCapabilities, PolicyCompiler,
};
use proptest::prelude::*;
use support::{app_match, domain_match, matcher, must_policy};

fn compile(
    version: u64,
    created_at: u64,
    policies: Vec<nonproxy_model::Policy>,
    capabilities: CompileCapabilities,
) -> Result<nonproxy_policy::CompiledPolicySnapshot, CompileError> {
    PolicyCompiler::compile(CompileRequest::new(
        version,
        created_at,
        DecisionSpec::direct(),
        policies,
        capabilities,
    ))
}

fn conflict_codes(error: &CompileError) -> Vec<&'static str> {
    error
        .conflicts()
        .iter()
        .map(|conflict| conflict.code())
        .collect()
}

fn network_profile(value: char) -> NetworkProfileBinding {
    NetworkProfileBinding::new(
        NetworkProfileId::new("office")
            .unwrap_or_else(|error| panic!("网络配置档标识创建失败: {error}")),
        NetworkFingerprint::new(
            NetworkFingerprintKind::WifiSsidSha256,
            value.to_string().repeat(64),
        )
        .unwrap_or_else(|error| panic!("网络配置档指纹创建失败: {error}")),
    )
}

fn network_policy() -> nonproxy_model::Policy {
    let profile_id = NetworkProfileId::new("office")
        .unwrap_or_else(|error| panic!("网络策略配置档标识创建失败: {error}"));
    must_policy(
        "office-policy",
        PolicySourceKind::Network,
        matcher(
            None,
            None,
            None,
            Some(NetworkMatcher::new(profile_id)),
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    )
}

#[test]
fn snapshot_version_zero_is_rejected() {
    let result = compile(0, 10, Vec::new(), CompileCapabilities::full());
    let Err(error) = result else {
        panic!("零版本快照不应编译成功");
    };

    assert!(conflict_codes(&error).contains(&"NP_POLICY_SNAPSHOT_VERSION_INVALID"));
}

#[test]
fn duplicate_policy_ids_are_rejected_even_when_one_is_disabled() {
    let first = must_policy(
        "duplicate",
        PolicySourceKind::App,
        matcher(
            Some(app_match("com.example.first")),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::direct(),
        0,
    );
    let second = must_policy(
        "duplicate",
        PolicySourceKind::App,
        matcher(
            Some(app_match("com.example.second")),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    )
    .disabled();
    let result = compile(1, 10, vec![first, second], CompileCapabilities::full());
    let Err(error) = result else {
        panic!("重复策略标识不应编译成功");
    };

    assert!(conflict_codes(&error).contains(&"NP_POLICY_DUPLICATE_ID"));
}

#[test]
fn identical_selector_at_same_priority_is_rejected() {
    let first = must_policy(
        "selector-a",
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
        5,
    );
    let second = must_policy(
        "selector-b",
        PolicySourceKind::Site,
        first.matcher().clone(),
        DecisionSpec::blocked(),
        5,
    );
    let result = compile(1, 10, vec![first, second], CompileCapabilities::full());
    let Err(error) = result else {
        panic!("歧义选择器不应编译成功");
    };

    assert!(conflict_codes(&error).contains(&"NP_POLICY_AMBIGUOUS_SELECTOR"));
}

#[test]
fn unsupported_match_dimension_is_not_silently_downgraded() {
    let policy = must_policy(
        "app-rule",
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
    let capabilities = CompileCapabilities::new(false, true, true, OutboundCapabilities::full());
    let result = compile(1, 10, vec![policy], capabilities);
    let Err(error) = result else {
        panic!("不支持的应用匹配不应被降级");
    };

    assert!(conflict_codes(&error).contains(&"NP_POLICY_CAPABILITY_APP_UNSUPPORTED"));
}

#[test]
fn proxy_outbound_must_exist_and_cover_required_protocols() {
    let outbound_id = match OutboundId::new("limited") {
        Ok(value) => value,
        Err(error) => panic!("测试出口标识创建失败: {error}"),
    };
    let decision = match DecisionSpec::new(
        RouteAction::Proxy,
        Some(outbound_id.clone()),
        FailureMode::Closed,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试代理决策创建失败: {error}"),
    };
    let policy = must_policy(
        "proxy-rule",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Suffix, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        decision,
        0,
    );
    let capabilities = CompileCapabilities::full().with_outbound(
        outbound_id,
        OutboundCapabilities::new(true, false, true, false),
    );
    let result = compile(1, 10, vec![policy], capabilities);
    let Err(error) = result else {
        panic!("能力不足的代理出口不应编译成功");
    };
    let codes = conflict_codes(&error);

    assert!(codes.contains(&"NP_POLICY_OUTBOUND_TRANSPORT_UNSUPPORTED"));
    assert!(codes.contains(&"NP_POLICY_OUTBOUND_IP_FAMILY_UNSUPPORTED"));
}

#[test]
fn content_hash_is_independent_of_input_order_and_snapshot_metadata() {
    let app = must_policy(
        "app-rule",
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
    let site = must_policy(
        "site-rule",
        PolicySourceKind::Site,
        matcher(
            None,
            Some(domain_match(DomainMatchKind::Exact, "example.com")),
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        DecisionSpec::blocked(),
        0,
    );
    let first = compile(
        1,
        10,
        vec![app.clone(), site.clone()],
        CompileCapabilities::full(),
    );
    let second = compile(99, 999, vec![site, app], CompileCapabilities::full());
    let (Ok(first), Ok(second)) = (first, second) else {
        panic!("确定性哈希测试快照编译失败");
    };

    assert_eq!(
        first.metadata().content_hash(),
        second.metadata().content_hash()
    );
}

#[test]
fn target_requires_at_least_one_transport_and_ip_family() {
    let capabilities = CompileCapabilities::new(
        true,
        true,
        true,
        OutboundCapabilities::new(false, false, false, false),
    );
    let result = compile(1, 10, Vec::new(), capabilities);
    let Err(error) = result else {
        panic!("空目标能力不应编译成功");
    };
    let codes = conflict_codes(&error);

    assert!(codes.contains(&"NP_POLICY_TARGET_TRANSPORT_EMPTY"));
    assert!(codes.contains(&"NP_POLICY_TARGET_IP_FAMILY_EMPTY"));
}

#[test]
fn disabled_policy_is_excluded_from_snapshot() {
    let policy = must_policy(
        "disabled",
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
    )
    .disabled();
    let snapshot = compile(1, 10, vec![policy], CompileCapabilities::full());
    let Ok(snapshot) = snapshot else {
        panic!("禁用策略不应阻止快照编译");
    };

    assert_eq!(snapshot.metadata().policy_count(), 0);
}

#[test]
fn outbound_capabilities_are_stored_and_affect_content_hash() {
    let outbound_id = match OutboundId::new("primary") {
        Ok(value) => value,
        Err(error) => panic!("测试出口标识创建失败: {error}"),
    };
    let full = compile(
        1,
        10,
        Vec::new(),
        CompileCapabilities::full()
            .with_outbound(outbound_id.clone(), OutboundCapabilities::full()),
    );
    let limited = compile(
        2,
        20,
        Vec::new(),
        CompileCapabilities::full().with_outbound(
            outbound_id.clone(),
            OutboundCapabilities::new(true, false, true, false),
        ),
    );
    let (Ok(full), Ok(limited)) = (full, limited) else {
        panic!("出口能力快照编译失败");
    };

    assert!(full.outbound_capabilities().contains_key(&outbound_id));
    assert_ne!(
        full.metadata().content_hash(),
        limited.metadata().content_hash()
    );
}

#[test]
fn network_profile_catalog_is_authoritative_and_affects_content_hash() {
    let policy = network_policy();
    let missing = PolicyCompiler::compile(
        CompileRequest::new(
            1,
            10,
            DecisionSpec::direct(),
            vec![policy.clone()],
            CompileCapabilities::full(),
        )
        .with_network_profiles(Vec::new()),
    );
    let Err(missing) = missing else {
        panic!("缺少网络配置档的网络规则不应编译成功");
    };
    assert!(conflict_codes(&missing).contains(&"NP_POLICY_NETWORK_PROFILE_MISSING"));

    let first = PolicyCompiler::compile(
        CompileRequest::new(
            1,
            10,
            DecisionSpec::direct(),
            vec![policy.clone()],
            CompileCapabilities::full(),
        )
        .with_network_profiles(vec![network_profile('a')]),
    );
    let second = PolicyCompiler::compile(
        CompileRequest::new(
            2,
            20,
            DecisionSpec::direct(),
            vec![policy],
            CompileCapabilities::full(),
        )
        .with_network_profiles(vec![network_profile('b')]),
    );
    let (Ok(first), Ok(second)) = (first, second) else {
        panic!("网络配置档快照编译失败");
    };
    assert!(
        first
            .network_profiles()
            .contains_key(&NetworkProfileId::new("office").unwrap_or_else(|error| {
                panic!("网络配置档结果标识创建失败: {error}")
            }))
    );
    assert_ne!(
        first.metadata().content_hash(),
        second.metadata().content_hash()
    );
}

proptest! {
    #[test]
    fn content_hash_is_deterministic_for_generated_priorities(
        app_priority in any::<i32>(),
        site_priority in any::<i32>(),
    ) {
        let app = must_policy(
            "property-app",
            PolicySourceKind::App,
            matcher(
                Some(app_match("com.example.property")),
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
            ),
            DecisionSpec::direct(),
            app_priority,
        );
        let site = must_policy(
            "property-site",
            PolicySourceKind::Site,
            matcher(
                None,
                Some(domain_match(
                    DomainMatchKind::Suffix,
                    "property.example",
                )),
                None,
                None,
                Vec::new(),
                Vec::new(),
            ),
            DecisionSpec::blocked(),
            site_priority,
        );
        let first = compile(
            1,
            10,
            vec![app.clone(), site.clone()],
            CompileCapabilities::full(),
        );
        let second = compile(
            2,
            20,
            vec![site, app],
            CompileCapabilities::full(),
        );

        match (first, second) {
            (Ok(first), Ok(second)) => {
                prop_assert_eq!(
                    first.metadata().content_hash(),
                    second.metadata().content_hash()
                );
            }
            (first, second) => {
                prop_assert!(
                    false,
                    "property 快照编译失败: {first:?}, {second:?}"
                );
            }
        }
    }
}
