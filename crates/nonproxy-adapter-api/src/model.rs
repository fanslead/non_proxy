use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::{Component, Path},
    str::FromStr,
};

use nonproxy_model::DomainName;
use serde::{Deserialize, Serialize};

use crate::AdapterContractError;

const POLICY_FORMAT_VERSION: u32 = 1;
const MAXIMUM_POLICY_BYTES: usize = 1024 * 1024;
const MAXIMUM_RULES: usize = 4_096;
const MAXIMUM_RULE_ID_BYTES: usize = 128;
const MAXIMUM_PATH_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterClient {
    Surge,
    Mihomo,
    SingBox,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterCapability {
    ApplicationRule,
    DomainRule,
    CidrRule,
    HotReload,
    PathEvidence,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AdapterVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl AdapterVersion {
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl FromStr for AdapterVersion {
    type Err = AdapterContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let core = value
            .trim()
            .split_once('-')
            .map_or(value.trim(), |(core, _)| core);
        let mut segments = core.split('.');
        let major = parse_segment(segments.next())?;
        let minor = parse_segment(segments.next())?;
        let patch = segments.next().map_or(Ok(0), |value| {
            value
                .parse::<u32>()
                .map_err(|_| AdapterContractError::ClientVersionUnsupported)
        })?;
        if segments.next().is_some() {
            return Err(AdapterContractError::ClientVersionUnsupported);
        }
        Ok(Self::new(major, minor, patch))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Direct,
    Proxy,
    Block,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainSelectorKind {
    Exact,
    Suffix,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleSelector {
    Application {
        bundle_path: String,
    },
    Domain {
        match_kind: DomainSelectorKind,
        value: String,
    },
    Cidr {
        value: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedRule {
    pub id: String,
    pub action: PolicyAction,
    pub selector: RuleSelector,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedPolicy {
    pub format_version: u32,
    pub revision: u64,
    pub rules: Vec<NormalizedRule>,
}

impl NormalizedPolicy {
    pub fn from_json(bytes: &[u8]) -> Result<Self, AdapterContractError> {
        if bytes.len() > MAXIMUM_POLICY_BYTES {
            return Err(AdapterContractError::PolicyTooLarge);
        }
        let mut policy: Self =
            serde_json::from_slice(bytes).map_err(|_| AdapterContractError::PolicyInvalid)?;
        policy.validate_and_normalize()?;
        Ok(policy)
    }

    pub fn validate_and_normalize(&mut self) -> Result<(), AdapterContractError> {
        if self.format_version != POLICY_FORMAT_VERSION {
            return Err(AdapterContractError::PolicyVersionUnsupported);
        }
        if self.revision == 0 {
            return Err(AdapterContractError::PolicyRevisionInvalid);
        }
        if self.rules.len() > MAXIMUM_RULES {
            return Err(AdapterContractError::RuleLimitExceeded);
        }
        let mut identifiers = BTreeSet::new();
        for rule in &mut self.rules {
            validate_rule_id(&rule.id)?;
            if !identifiers.insert(rule.id.clone()) {
                return Err(AdapterContractError::DuplicateRuleId);
            }
            if rule.action != PolicyAction::Direct {
                return Err(AdapterContractError::ActionUnsupported);
            }
            normalize_selector(&mut rule.selector)?;
        }
        self.rules.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(())
    }
}

fn parse_segment(value: Option<&str>) -> Result<u32, AdapterContractError> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(AdapterContractError::ClientVersionUnsupported)?
        .parse::<u32>()
        .map_err(|_| AdapterContractError::ClientVersionUnsupported)
}

fn validate_rule_id(value: &str) -> Result<(), AdapterContractError> {
    if value.is_empty()
        || value.len() > MAXIMUM_RULE_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AdapterContractError::RuleIdInvalid);
    }
    Ok(())
}

fn normalize_selector(selector: &mut RuleSelector) -> Result<(), AdapterContractError> {
    match selector {
        RuleSelector::Application { bundle_path } => {
            let trimmed = bundle_path.trim_end_matches('/');
            let path = Path::new(trimmed);
            if trimmed.is_empty()
                || trimmed.len() > MAXIMUM_PATH_BYTES
                || !trimmed.starts_with('/')
                || !trimmed.ends_with(".app")
                || trimmed
                    .split('/')
                    .any(|component| matches!(component, "." | ".."))
                || path
                    .components()
                    .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
                || trimmed
                    .chars()
                    .any(|character| character.is_control() || matches!(character, ',' | '\0'))
            {
                return Err(AdapterContractError::SelectorInvalid);
            }
            *bundle_path = format!("{trimmed}/");
            Ok(())
        }
        RuleSelector::Domain { value, .. } => {
            let normalized =
                DomainName::normalize(value).map_err(|_| AdapterContractError::SelectorInvalid)?;
            *value = normalized.as_ascii().to_owned();
            Ok(())
        }
        RuleSelector::Cidr { value } => {
            let Some((address, prefix)) = value.split_once('/') else {
                return Err(AdapterContractError::SelectorInvalid);
            };
            let address = address
                .parse::<IpAddr>()
                .map_err(|_| AdapterContractError::SelectorInvalid)?;
            let prefix = prefix
                .parse::<u8>()
                .map_err(|_| AdapterContractError::SelectorInvalid)?;
            let maximum = if address.is_ipv4() { 32 } else { 128 };
            if prefix > maximum {
                return Err(AdapterContractError::SelectorInvalid);
            }
            *value = canonical_cidr(address, prefix);
            Ok(())
        }
    }
}

fn canonical_cidr(address: IpAddr, prefix: u8) -> String {
    match address {
        IpAddr::V4(address) => {
            let bits = u32::from(address);
            let mask = u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);
            format!("{}/{prefix}", Ipv4Addr::from(bits & mask))
        }
        IpAddr::V6(address) => {
            let bits = u128::from(address);
            let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);
            format!("{}/{prefix}", Ipv6Addr::from(bits & mask))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn policy_normalizes_and_sorts_safe_selectors() {
        let mut policy = NormalizedPolicy {
            format_version: 1,
            revision: 8,
            rules: vec![
                NormalizedRule {
                    id: "site-z".to_owned(),
                    action: PolicyAction::Direct,
                    selector: RuleSelector::Domain {
                        match_kind: DomainSelectorKind::Suffix,
                        value: "例子.测试".to_owned(),
                    },
                },
                NormalizedRule {
                    id: "app-a".to_owned(),
                    action: PolicyAction::Direct,
                    selector: RuleSelector::Application {
                        bundle_path: "/Applications/ChatGPT.app".to_owned(),
                    },
                },
            ],
        };

        assert_eq!(policy.validate_and_normalize(), Ok(()));
        assert_eq!(policy.rules[0].id, "app-a");
        assert!(matches!(
            &policy.rules[0].selector,
            RuleSelector::Application { bundle_path }
                if bundle_path == "/Applications/ChatGPT.app/"
        ));
        assert!(matches!(
            &policy.rules[1].selector,
            RuleSelector::Domain { value, .. } if value == "xn--fsqu00a.xn--0zwm56d"
        ));
    }

    #[test]
    fn policy_rejects_injection_and_unsupported_action() {
        let injected = br#"{
            "format_version":1,
            "revision":1,
            "rules":[{"id":"app","action":"direct","selector":{
              "kind":"application","bundle_path":"/Applications/App.app/,FINAL,PROXY"
            }}]
        }"#;
        let proxy = br#"{
            "format_version":1,
            "revision":1,
            "rules":[{"id":"site","action":"proxy","selector":{
              "kind":"domain","match_kind":"exact","value":"example.com"
            }}]
        }"#;

        assert_eq!(
            NormalizedPolicy::from_json(injected),
            Err(AdapterContractError::SelectorInvalid)
        );
        assert_eq!(
            NormalizedPolicy::from_json(proxy),
            Err(AdapterContractError::ActionUnsupported)
        );
    }

    #[test]
    fn versions_are_strict_and_orderable() {
        assert_eq!(
            AdapterVersion::from_str("1.11.3-beta.1"),
            Ok(AdapterVersion::new(1, 11, 3))
        );
        assert_eq!(
            AdapterVersion::from_str("6.0"),
            Ok(AdapterVersion::new(6, 0, 0))
        );
        assert_eq!(
            AdapterVersion::from_str("latest"),
            Err(AdapterContractError::ClientVersionUnsupported)
        );
    }

    #[test]
    fn cidr_host_bits_are_canonicalized() {
        let mut policy = NormalizedPolicy {
            format_version: 1,
            revision: 1,
            rules: vec![NormalizedRule {
                id: "network".to_owned(),
                action: PolicyAction::Direct,
                selector: RuleSelector::Cidr {
                    value: "10.12.13.14/8".to_owned(),
                },
            }],
        };

        assert_eq!(policy.validate_and_normalize(), Ok(()));
        assert!(matches!(
            &policy.rules[0].selector,
            RuleSelector::Cidr { value } if value == "10.0.0.0/8"
        ));
    }

    #[test]
    fn application_path_rejects_relative_components() {
        for bundle_path in [
            "/Applications/../Private/App.app",
            "/Applications/./App.app",
        ] {
            let mut policy = NormalizedPolicy {
                format_version: 1,
                revision: 1,
                rules: vec![NormalizedRule {
                    id: "app".to_owned(),
                    action: PolicyAction::Direct,
                    selector: RuleSelector::Application {
                        bundle_path: bundle_path.to_owned(),
                    },
                }],
            };

            assert_eq!(
                policy.validate_and_normalize(),
                Err(AdapterContractError::SelectorInvalid)
            );
        }
    }

    proptest! {
        #[test]
        fn application_path_rejects_every_ascii_control(control in 0_u8..=31) {
            let mut policy = NormalizedPolicy {
                format_version: 1,
                revision: 1,
                rules: vec![NormalizedRule {
                    id: "app".to_owned(),
                    action: PolicyAction::Direct,
                    selector: RuleSelector::Application {
                        bundle_path: format!(
                            "/Applications/Safe{}Name.app",
                            char::from(control)
                        ),
                    },
                }],
            };

            prop_assert_eq!(
                policy.validate_and_normalize(),
                Err(AdapterContractError::SelectorInvalid)
            );
        }
    }
}
