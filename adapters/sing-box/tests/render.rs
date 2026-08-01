use nonproxy_adapter_api::{AdapterRenderer, AdapterVersion, NormalizedPolicy};
use nonproxy_adapter_sing_box::SingBoxRenderer;

#[test]
fn public_fixture_renders_without_secrets() {
    let policy = NormalizedPolicy::from_json(include_bytes!("../fixtures/direct-policy-v2.json"))
        .unwrap_or_else(|error| panic!("公开 fixture 无效: {error}"));
    let rendered = SingBoxRenderer
        .render(AdapterVersion::new(1, 11, 0), &policy)
        .unwrap_or_else(|error| panic!("sing-box fixture 渲染失败: {error}"));

    assert_eq!(rendered.rule_count(), 2);
    assert!(
        !rendered
            .bytes()
            .windows(8)
            .any(|value| value == b"password")
    );
}
