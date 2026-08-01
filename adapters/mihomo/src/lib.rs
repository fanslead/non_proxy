use nonproxy_adapter_api::{
    AdapterCapability, AdapterClient, AdapterContractError, AdapterRenderer, AdapterVersion,
    ApplicationPathKind, ApplicationSelectorPlatform, DomainSelectorKind, NormalizedPolicy,
    RenderedRules, RuleSelector,
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
            AdapterCapability::HotReload,
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
                RuleSelector::Application {
                    platform: ApplicationSelectorPlatform::Macos,
                    path_kind: ApplicationPathKind::Bundle,
                    value,
                    ..
                } => format!("PROCESS-PATH-WILDCARD,{value}*"),
                RuleSelector::Application {
                    platform: ApplicationSelectorPlatform::Windows,
                    path_kind: ApplicationPathKind::Executable,
                    value,
                    ..
                } => format!("PROCESS-PATH,{value}"),
                RuleSelector::Application { .. } => {
                    return Err(AdapterContractError::SelectorInvalid);
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
    use nonproxy_adapter_api::{
        AdapterContractError, AdapterRenderer, AdapterVersion, NormalizedPolicy,
    };

    use super::MihomoRenderer;

    #[test]
    fn renders_quoted_classical_provider() {
        let policy = NormalizedPolicy::from_json(
            br#"{
              "format_version":2,"revision":2,"rules":[
                {"id":"site","action":"direct","selector":{"kind":"domain","match_kind":"exact","value":"api.example.com"}},
                {"id":"app","action":"direct","selector":{"kind":"application","selector_version":1,"platform":"macos","path_kind":"bundle","value":"/Applications/Worker's App.app"}},
                {"id":"windows","action":"direct","selector":{"kind":"application","selector_version":1,"platform":"windows","path_kind":"executable","value":"C:\\Program Files\\Chat\\chat.exe"}},
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
        assert!(text.contains("'PROCESS-PATH,C:\\Program Files\\Chat\\chat.exe'"));
        assert!(text.contains("'DOMAIN,api.example.com'"));
        assert!(text.contains("'IP-CIDR6,2001:db8::/32,no-resolve'"));
    }

    #[test]
    fn windows_package_family_is_not_downgraded_to_process_name() {
        let policy = NormalizedPolicy::from_json(
            br#"{"format_version":2,"revision":1,"rules":[
              {"id":"package","action":"direct","selector":{
                "kind":"application","selector_version":1,"platform":"windows",
                "path_kind":"package_family","value":"Example.Chat_1234567890abc"
              }}
            ]}"#,
        )
        .unwrap_or_else(|error| panic!("测试策略无效: {error}"));

        assert_eq!(
            MihomoRenderer.render(AdapterVersion::new(1, 19, 1), &policy),
            Err(AdapterContractError::SelectorInvalid)
        );
    }
}
