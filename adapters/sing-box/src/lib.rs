use nonproxy_adapter_api::{
    AdapterCapability, AdapterClient, AdapterContractError, AdapterRenderer, AdapterVersion,
    DomainSelectorKind, NormalizedPolicy, RenderedRules, RuleSelector,
};
use serde_json::{Map, Value, json};

const MINIMUM_SUPPORTED_VERSION: AdapterVersion = AdapterVersion::new(1, 11, 0);
const SOURCE_FORMAT_VERSION: u32 = 3;
const OUTPUT_FORMAT: &str = "sing-box-source-rule-set-v3";

#[derive(Clone, Copy, Debug, Default)]
pub struct SingBoxRenderer;

impl AdapterRenderer for SingBoxRenderer {
    fn client(&self) -> AdapterClient {
        AdapterClient::SingBox
    }

    fn capabilities(&self, version: AdapterVersion) -> Vec<AdapterCapability> {
        if version < MINIMUM_SUPPORTED_VERSION {
            return Vec::new();
        }
        vec![
            AdapterCapability::ApplicationRule,
            AdapterCapability::DomainRule,
            AdapterCapability::CidrRule,
        ]
    }

    fn render(
        &self,
        version: AdapterVersion,
        policy: &NormalizedPolicy,
    ) -> Result<RenderedRules, AdapterContractError> {
        if version < MINIMUM_SUPPORTED_VERSION {
            return Err(AdapterContractError::ClientVersionUnsupported);
        }
        let rules = policy
            .rules
            .iter()
            .map(|rule| match &rule.selector {
                RuleSelector::Application { bundle_path } => json!({
                    "process_path_regex": [format!("^{}", escape_regex(bundle_path))]
                }),
                RuleSelector::Domain { match_kind, value } => match match_kind {
                    DomainSelectorKind::Exact => json!({ "domain": [value] }),
                    DomainSelectorKind::Suffix => json!({ "domain_suffix": [value] }),
                },
                RuleSelector::Cidr { value } => json!({ "ip_cidr": [value] }),
            })
            .collect::<Vec<_>>();
        let mut root = Map::new();
        root.insert("version".to_owned(), json!(SOURCE_FORMAT_VERSION));
        root.insert("rules".to_owned(), Value::Array(rules));
        let mut bytes = serde_json::to_vec_pretty(&Value::Object(root))
            .map_err(|_| AdapterContractError::PolicyInvalid)?;
        bytes.push(b'\n');
        RenderedRules::new(self.client(), OUTPUT_FORMAT, bytes, policy.rules.len())
    }
}

fn escape_regex(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}

#[cfg(test)]
mod tests {
    use nonproxy_adapter_api::{AdapterRenderer, AdapterVersion, NormalizedPolicy};
    use serde_json::Value;

    use super::SingBoxRenderer;

    #[test]
    fn renders_source_rule_set_v3_without_route_action() {
        let policy = NormalizedPolicy::from_json(
            br#"{
              "format_version":1,"revision":2,"rules":[
                {"id":"site","action":"direct","selector":{"kind":"domain","match_kind":"suffix","value":"example.com"}},
                {"id":"app","action":"direct","selector":{"kind":"application","bundle_path":"/Applications/App (Beta).app"}},
                {"id":"lan","action":"direct","selector":{"kind":"cidr","value":"192.168.0.0/16"}}
              ]
            }"#,
        )
        .unwrap_or_else(|error| panic!("测试策略无效: {error}"));

        let rendered = SingBoxRenderer
            .render(AdapterVersion::new(1, 11, 0), &policy)
            .unwrap_or_else(|error| panic!("sing-box 规则渲染失败: {error}"));
        let parsed: Value = serde_json::from_slice(rendered.bytes())
            .unwrap_or_else(|error| panic!("渲染结果不是 JSON: {error}"));

        assert_eq!(parsed["version"], 3);
        assert_eq!(parsed["rules"].as_array().map(Vec::len), Some(3));
        assert_eq!(
            parsed["rules"][0]["process_path_regex"][0],
            "^/Applications/App \\(Beta\\)\\.app/"
        );
        assert_eq!(parsed["rules"][1]["ip_cidr"][0], "192.168.0.0/16");
        assert_eq!(parsed["rules"][2]["domain_suffix"][0], "example.com");
        assert!(parsed["rules"][0].get("action").is_none());
    }
}
