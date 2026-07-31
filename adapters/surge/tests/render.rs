use nonproxy_adapter_api::{AdapterRenderer, AdapterVersion, NormalizedPolicy};
use nonproxy_adapter_surge::SurgeRenderer;

#[test]
fn public_fixture_renders_without_secrets() {
    let policy = NormalizedPolicy::from_json(include_bytes!("../fixtures/direct-policy-v1.json"))
        .unwrap_or_else(|error| panic!("公开 fixture 无效: {error}"));
    let rendered = SurgeRenderer
        .render(AdapterVersion::new(6, 0, 0), &policy)
        .unwrap_or_else(|error| panic!("Surge fixture 渲染失败: {error}"));

    assert_eq!(rendered.rule_count(), 2);
    assert!(
        !rendered
            .bytes()
            .windows(8)
            .any(|value| value == b"password")
    );
}
