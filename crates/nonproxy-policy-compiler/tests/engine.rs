mod support;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use nonproxy_model::{
    AppIdentity, ConnectionContext, DecisionSpec, DomainMatchKind, Platform, Policy,
    PolicySourceKind, Transport,
};
use nonproxy_policy::{CompiledPolicySnapshot, PolicyEngine};
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
