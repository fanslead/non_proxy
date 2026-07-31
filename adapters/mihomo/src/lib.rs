use nonproxy_adapter_api::{
    AdapterCapability, AdapterClient, AdapterContractError, AdapterRenderer, AdapterVersion,
    DomainSelectorKind, NormalizedPolicy, RenderedRules, RuleSelector,
};

const MINIMUM_SUPPORTED_VERSION: AdapterVersion = AdapterVersion::new(1, 18, 0);
const OUTPUT_FORMAT: &str = "mihomo-classical-provider-v1";

#[derive(Clone, Copy, Debug, Default)]
pub struct MihomoRenderer;

impl AdapterRenderer for MihomoRenderer {
    fn client(&self) -> AdapterClient {
        AdapterClient::Mihomo
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
        let mut output = String::from("# NonProxy managed classical rule provider.\npayload:\n");
        for rule in &policy.rules {
            let value = match &rule.selector {
                RuleSelector::Application { bundle_path } => {
                    format!("PROCESS-PATH-WILDCARD,{bundle_path}*")
                }
                RuleSelector::Domain { match_kind, value } => match match_kind {
                    DomainSelectorKind::Exact => format!("DOMAIN,{value}"),
                    DomainSelectorKind::Suffix => format!("DOMAIN-SUFFIX,{value}"),
                },
                RuleSelector::Cidr { value } => {
                    let kind = if value.contains(':') {
                        "IP-CIDR6"
                    } else {
                        "IP-CIDR"
                    };
                    format!("{kind},{value},no-resolve")
                }
            };
            output.push_str("  - '");
            output.push_str(&value.replace('\'', "''"));
            output.push_str("'\n");
        }
        RenderedRules::new(
            self.client(),
            OUTPUT_FORMAT,
            output.into_bytes(),
            policy.rules.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_adapter_api::{AdapterRenderer, AdapterVersion, NormalizedPolicy};

    use super::MihomoRenderer;

    #[test]
    fn renders_quoted_classical_provider() {
        let policy = NormalizedPolicy::from_json(
            br#"{
              "format_version":1,"revision":2,"rules":[
                {"id":"site","action":"direct","selector":{"kind":"domain","match_kind":"exact","value":"api.example.com"}},
                {"id":"app","action":"direct","selector":{"kind":"application","bundle_path":"/Applications/Worker's App.app"}},
                {"id":"v6","action":"direct","selector":{"kind":"cidr","value":"2001:db8::/32"}}
              ]
            }"#,
        )
        .unwrap_or_else(|error| panic!("测试策略无效: {error}"));

        let rendered = MihomoRenderer
            .render(AdapterVersion::new(1, 19, 1), &policy)
            .unwrap_or_else(|error| panic!("Mihomo 规则渲染失败: {error}"));
        let text = String::from_utf8_lossy(rendered.bytes());

        assert!(text.starts_with("# NonProxy managed classical rule provider.\npayload:\n"));
        assert!(text.contains("PROCESS-PATH-WILDCARD,/Applications/Worker''s App.app/*"));
        assert!(text.contains("'DOMAIN,api.example.com'"));
        assert!(text.contains("'IP-CIDR6,2001:db8::/32,no-resolve'"));
    }
}
