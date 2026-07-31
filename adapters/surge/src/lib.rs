use nonproxy_adapter_api::{
    AdapterCapability, AdapterClient, AdapterContractError, AdapterRenderer, AdapterVersion,
    DomainSelectorKind, NormalizedPolicy, RenderedRules, RuleSelector,
};

const MINIMUM_SUPPORTED_VERSION: AdapterVersion = AdapterVersion::new(5, 0, 0);
const APP_BUNDLE_RULE_VERSION: AdapterVersion = AdapterVersion::new(6, 0, 0);
const OUTPUT_FORMAT: &str = "surge-ruleset-v1";

#[derive(Clone, Copy, Debug, Default)]
pub struct SurgeRenderer;

impl AdapterRenderer for SurgeRenderer {
    fn client(&self) -> AdapterClient {
        AdapterClient::Surge
    }

    fn capabilities(&self, version: AdapterVersion) -> Vec<AdapterCapability> {
        if version < MINIMUM_SUPPORTED_VERSION {
            return Vec::new();
        }
        let mut capabilities = vec![AdapterCapability::DomainRule, AdapterCapability::CidrRule];
        if version >= APP_BUNDLE_RULE_VERSION {
            capabilities.push(AdapterCapability::ApplicationRule);
        }
        capabilities
    }

    fn render(
        &self,
        version: AdapterVersion,
        policy: &NormalizedPolicy,
    ) -> Result<RenderedRules, AdapterContractError> {
        if version < MINIMUM_SUPPORTED_VERSION {
            return Err(AdapterContractError::ClientVersionUnsupported);
        }
        let mut output =
            String::from("# NonProxy managed ruleset. Do not edit this generated file.\n");
        for rule in &policy.rules {
            let line = match &rule.selector {
                RuleSelector::Application { bundle_path } => {
                    if version < APP_BUNDLE_RULE_VERSION {
                        return Err(AdapterContractError::ClientVersionUnsupported);
                    }
                    format!("PROCESS-NAME,{bundle_path}")
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
            output.push_str(&line);
            output.push('\n');
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
    use nonproxy_adapter_api::{
        AdapterContractError, AdapterRenderer, AdapterVersion, NormalizedPolicy,
    };

    use super::SurgeRenderer;

    #[test]
    fn renders_deterministic_external_ruleset() {
        let policy = NormalizedPolicy::from_json(
            br#"{
              "format_version":1,"revision":2,"rules":[
                {"id":"site","action":"direct","selector":{"kind":"domain","match_kind":"suffix","value":"example.com"}},
                {"id":"app","action":"direct","selector":{"kind":"application","bundle_path":"/Applications/ChatGPT.app"}},
                {"id":"lan","action":"direct","selector":{"kind":"cidr","value":"10.0.0.0/8"}}
              ]
            }"#,
        )
        .unwrap_or_else(|error| panic!("测试策略无效: {error}"));

        let rendered = SurgeRenderer
            .render(AdapterVersion::new(6, 0, 0), &policy)
            .unwrap_or_else(|error| panic!("Surge 规则渲染失败: {error}"));
        let text = String::from_utf8_lossy(rendered.bytes());

        assert_eq!(rendered.rule_count(), 3);
        assert!(text.contains("PROCESS-NAME,/Applications/ChatGPT.app/\n"));
        assert!(text.contains("IP-CIDR,10.0.0.0/8,no-resolve\n"));
        assert!(text.contains("DOMAIN-SUFFIX,example.com\n"));
        assert!(!text.contains(",DIRECT"));
    }

    #[test]
    fn old_client_refuses_bundle_prefix_rule() {
        let policy = NormalizedPolicy::from_json(
            br#"{"format_version":1,"revision":1,"rules":[
              {"id":"app","action":"direct","selector":{"kind":"application","bundle_path":"/Applications/App.app"}}
            ]}"#,
        )
        .unwrap_or_else(|error| panic!("测试策略无效: {error}"));

        assert_eq!(
            SurgeRenderer.render(AdapterVersion::new(5, 9, 0), &policy),
            Err(AdapterContractError::ClientVersionUnsupported)
        );
    }
}
