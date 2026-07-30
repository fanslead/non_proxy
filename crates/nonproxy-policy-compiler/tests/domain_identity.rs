mod support;

use nonproxy_model::{
    AppMatcher, DecisionSpec, DomainMatchKind, DomainName, Platform, Policy, PolicyMetadata,
    PolicyOrigin, PolicySourceKind,
};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};

use support::{domain_match, matcher, must_policy};

#[test]
fn snapshot_reports_domain_identity_for_site_rule() {
    let site = must_policy(
        "site",
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
        10,
    );
    let Ok(snapshot) = PolicyCompiler::compile(CompileRequest::new(
        1,
        1_000,
        DecisionSpec::direct(),
        vec![site],
        CompileCapabilities::full(),
    )) else {
        panic!("站点策略应当编译成功");
    };

    assert!(snapshot.requires_domain_identity(&domain("api.example.com")));
    assert!(!snapshot.requires_domain_identity(&domain("notexample.com")));
}

#[test]
fn app_destination_rule_requires_domain_identity_before_app_is_known() {
    let app = AppMatcher::new(Platform::Windows, r"c:\apps\browser.exe")
        .unwrap_or_else(|error| panic!("测试应用匹配器无效: {error}"));
    let match_result = matcher(
        Some(app),
        Some(domain_match(DomainMatchKind::Exact, "private.example")),
        None,
        None,
        Vec::new(),
        Vec::new(),
    );
    let policy_result = Policy::new(
        nonproxy_model::PolicyId::new("app-site")
            .unwrap_or_else(|error| panic!("测试策略 ID 无效: {error}")),
        "应用站点",
        match_result,
        DecisionSpec::direct(),
        PolicyMetadata::new(PolicySourceKind::AppDestination, 10, PolicyOrigin::User, 1),
    );
    let policy = policy_result.unwrap_or_else(|error| panic!("应用站点策略无效: {error}"));
    let snapshot = PolicyCompiler::compile(CompileRequest::new(
        2,
        2_000,
        DecisionSpec::direct(),
        vec![policy],
        CompileCapabilities::full(),
    ))
    .unwrap_or_else(|error| panic!("应用站点策略应当编译成功: {error}"));

    assert!(snapshot.requires_domain_identity(&domain("private.example")));
    assert!(!snapshot.requires_domain_identity(&domain("other.example")));
}

fn domain(value: &str) -> DomainName {
    DomainName::normalize(value).unwrap_or_else(|error| panic!("测试域名无效: {error}"))
}
