use serde_yaml_ng::{Mapping, Value};

use crate::{AdapterIntegrationError, model::reference_name};

const DIRECT_TARGET: &str = "DIRECT";

pub(crate) fn patch(
    configuration: &[u8],
    integration_id: &str,
    managed_reference: &str,
    direct_target: Option<&str>,
) -> Result<(Vec<u8>, String), AdapterIntegrationError> {
    validate_direct_target(direct_target)?;
    let input = std::str::from_utf8(configuration)
        .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)?;
    validate_unique_critical_keys(input)?;
    let name = reference_name(integration_id);
    let mut output = input.to_owned();
    let semantic = parse(&output)?;
    let provider_present = inspect_provider(&semantic, &name, managed_reference)?;
    if !provider_present {
        ensure_supported_key_syntax(&semantic, &output, "rule-providers")?;
        output = install_provider(&output, &name, managed_reference)?;
    }
    let semantic = parse(&output)?;
    let route_present = inspect_route(&semantic, &name)?;
    if !route_present {
        ensure_supported_key_syntax(&semantic, &output, "rules")?;
        output = install_route(&output, &name)?;
    }
    let (integrated, target) = inspect(
        output.as_bytes(),
        integration_id,
        managed_reference,
        direct_target,
    )?;
    if !integrated {
        return Err(AdapterIntegrationError::ConfigurationInvalid);
    }
    Ok((output.into_bytes(), target))
}

pub(crate) fn inspect(
    configuration: &[u8],
    integration_id: &str,
    managed_reference: &str,
    direct_target: Option<&str>,
) -> Result<(bool, String), AdapterIntegrationError> {
    validate_direct_target(direct_target)?;
    let input = std::str::from_utf8(configuration)
        .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)?;
    validate_unique_critical_keys(input)?;
    let semantic = parse(input)?;
    let name = reference_name(integration_id);
    Ok((
        inspect_provider(&semantic, &name, managed_reference)? && inspect_route(&semantic, &name)?,
        DIRECT_TARGET.to_owned(),
    ))
}

fn parse(input: &str) -> Result<Mapping, AdapterIntegrationError> {
    let value: Value = serde_yaml_ng::from_str(input)
        .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)?;
    value
        .as_mapping()
        .cloned()
        .ok_or(AdapterIntegrationError::ConfigurationInvalid)
}

fn validate_unique_critical_keys(input: &str) -> Result<(), AdapterIntegrationError> {
    if semantic_top_level_key_count(input, "rule-providers") > 1
        || semantic_top_level_key_count(input, "rules") > 1
    {
        return Err(AdapterIntegrationError::IntegrationConflict);
    }
    Ok(())
}

fn ensure_supported_key_syntax(
    root: &Mapping,
    input: &str,
    key: &str,
) -> Result<(), AdapterIntegrationError> {
    if mapping_value(root, key).is_some() && top_level_key_locations(input, key).len() != 1 {
        return Err(AdapterIntegrationError::IntegrationConflict);
    }
    Ok(())
}

fn inspect_provider(
    root: &Mapping,
    name: &str,
    managed_reference: &str,
) -> Result<bool, AdapterIntegrationError> {
    let Some(providers) = mapping_value(root, "rule-providers") else {
        return Ok(false);
    };
    let providers = providers
        .as_mapping()
        .ok_or(AdapterIntegrationError::IntegrationConflict)?;
    let Some(provider) = providers.get(Value::String(name.to_owned())) else {
        return Ok(false);
    };
    let provider = provider
        .as_mapping()
        .ok_or(AdapterIntegrationError::IntegrationConflict)?;
    let matches = provider.len() == 3
        && string_value(provider, "type") == Some("file")
        && string_value(provider, "behavior") == Some("classical")
        && string_value(provider, "path") == Some(managed_reference);
    if !matches {
        return Err(AdapterIntegrationError::IntegrationConflict);
    }
    Ok(true)
}

fn inspect_route(root: &Mapping, name: &str) -> Result<bool, AdapterIntegrationError> {
    let Some(rules) = mapping_value(root, "rules") else {
        return Ok(false);
    };
    let rules = rules
        .as_sequence()
        .ok_or(AdapterIntegrationError::IntegrationConflict)?;
    let expected = route_rule(name);
    let mut exact = false;
    for value in rules.iter().filter_map(Value::as_str) {
        if value == expected {
            exact = true;
        } else if rule_references_name(value, name) {
            return Err(AdapterIntegrationError::IntegrationConflict);
        }
    }
    Ok(exact)
}

fn install_provider(
    input: &str,
    name: &str,
    managed_reference: &str,
) -> Result<String, AdapterIntegrationError> {
    let line_ending = line_ending(input);
    let locations = top_level_key_locations(input, "rule-providers");
    match locations.as_slice() {
        [] => {
            let block = format!(
                "rule-providers:{line_ending}  {name}:{line_ending}    type: file{line_ending}    behavior: classical{line_ending}    path: {managed_reference}{line_ending}"
            );
            let insert_at = top_level_key_locations(input, "rules")
                .first()
                .map_or(input.len(), |location| location.line_start);
            Ok(insert_at_offset(input, insert_at, &block, line_ending))
        }
        [location] => {
            if !location.inline_value.is_empty() {
                if location.inline_value != "{}" {
                    return Err(AdapterIntegrationError::IntegrationConflict);
                }
                let replacement = format!(
                    "rule-providers:{line_ending}  {name}:{line_ending}    type: file{line_ending}    behavior: classical{line_ending}    path: {managed_reference}{line_ending}"
                );
                return Ok(replace_range(
                    input,
                    location.line_start,
                    location.line_end,
                    &replacement,
                ));
            }
            let end = top_level_block_end(input, location.line_end);
            let indentation = child_indentation(input, location.line_end, end).unwrap_or(2);
            if indentation > 16 {
                return Err(AdapterIntegrationError::IntegrationConflict);
            }
            let child = " ".repeat(indentation);
            let field = " ".repeat(indentation + 2);
            let block = format!(
                "{child}{name}:{line_ending}{field}type: file{line_ending}{field}behavior: classical{line_ending}{field}path: {managed_reference}{line_ending}"
            );
            Ok(insert_at_offset(input, end, &block, line_ending))
        }
        _ => Err(AdapterIntegrationError::IntegrationConflict),
    }
}

fn install_route(input: &str, name: &str) -> Result<String, AdapterIntegrationError> {
    let line_ending = line_ending(input);
    let locations = top_level_key_locations(input, "rules");
    let rule = route_rule(name);
    match locations.as_slice() {
        [] => {
            let prefix = if input.is_empty() || input.ends_with('\n') {
                String::new()
            } else {
                line_ending.to_owned()
            };
            Ok(format!(
                "{input}{prefix}rules:{line_ending}  - {rule}{line_ending}"
            ))
        }
        [location] => {
            if !location.inline_value.is_empty() {
                if location.inline_value != "[]" {
                    return Err(AdapterIntegrationError::IntegrationConflict);
                }
                let replacement = format!("rules:{line_ending}  - {rule}{line_ending}");
                return Ok(replace_range(
                    input,
                    location.line_start,
                    location.line_end,
                    &replacement,
                ));
            }
            let end = top_level_block_end(input, location.line_end);
            let indentation = child_indentation(input, location.line_end, end).unwrap_or(2);
            if indentation > 16 {
                return Err(AdapterIntegrationError::IntegrationConflict);
            }
            let route = format!("{}- {rule}{line_ending}", " ".repeat(indentation));
            Ok(insert_at_offset(
                input,
                location.line_end,
                &route,
                line_ending,
            ))
        }
        _ => Err(AdapterIntegrationError::IntegrationConflict),
    }
}

#[derive(Clone, Debug)]
struct KeyLocation {
    line_start: usize,
    line_end: usize,
    inline_value: String,
}

fn top_level_key_locations(input: &str, key: &str) -> Vec<KeyLocation> {
    line_ranges(input)
        .filter_map(|(start, end, line)| {
            if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
                return None;
            }
            let content = line.trim_end_matches(['\r', '\n']);
            let (candidate, value) = content.split_once(':')?;
            (candidate == key).then(|| KeyLocation {
                line_start: start,
                line_end: end,
                inline_value: value.trim().to_owned(),
            })
        })
        .collect()
}

fn semantic_top_level_key_count(input: &str, key: &str) -> usize {
    line_ranges(input)
        .filter(|(_, _, line)| {
            if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
                return false;
            }
            let content = line.trim_end_matches(['\r', '\n']);
            let Some((candidate, _)) = content.split_once(':') else {
                return false;
            };
            serde_yaml_ng::from_str::<Value>(candidate.trim())
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .is_some_and(|value| value == key)
        })
        .count()
}

fn top_level_block_end(input: &str, content_start: usize) -> usize {
    line_ranges(&input[content_start..])
        .find_map(|(start, _, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !line.starts_with(char::is_whitespace))
            .then_some(content_start + start)
        })
        .unwrap_or(input.len())
}

fn child_indentation(input: &str, start: usize, end: usize) -> Option<usize> {
    line_ranges(&input[start..end]).find_map(|(_, _, line)| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }
        let width = line.len() - line.trim_start_matches(' ').len();
        (width > 0 && !line.starts_with('\t')).then_some(width)
    })
}

fn insert_at_offset(input: &str, offset: usize, value: &str, line_ending: &str) -> String {
    let mut output = String::with_capacity(input.len() + value.len() + line_ending.len());
    output.push_str(&input[..offset]);
    if offset > 0 && !input[..offset].ends_with('\n') {
        output.push_str(line_ending);
    }
    output.push_str(value);
    output.push_str(&input[offset..]);
    output
}

fn replace_range(input: &str, start: usize, end: usize, value: &str) -> String {
    let mut output = String::with_capacity(input.len() + value.len());
    output.push_str(&input[..start]);
    output.push_str(value);
    output.push_str(&input[end..]);
    output
}

fn line_ranges(input: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut offset = 0_usize;
    input.split_inclusive('\n').map(move |line| {
        let start = offset;
        offset += line.len();
        (start, offset, line)
    })
}

fn line_ending(input: &str) -> &'static str {
    if input.contains("\r\n") { "\r\n" } else { "\n" }
}

fn mapping_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_owned()))
}

fn string_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a str> {
    mapping_value(mapping, key).and_then(Value::as_str)
}

fn route_rule(name: &str) -> String {
    format!("RULE-SET,{name},{DIRECT_TARGET}")
}

fn rule_references_name(rule: &str, name: &str) -> bool {
    let mut fields = rule.split(',').map(str::trim);
    fields.next().is_some_and(|value| value == "RULE-SET")
        && fields.next().is_some_and(|value| value == name)
}

fn validate_direct_target(value: Option<&str>) -> Result<(), AdapterIntegrationError> {
    if value.is_some_and(|value| !value.eq_ignore_ascii_case(DIRECT_TARGET)) {
        return Err(AdapterIntegrationError::DirectTargetInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::AdapterIntegrationError;

    use super::{inspect, patch};

    #[test]
    fn preserves_comments_and_inserts_provider_and_first_rule() {
        let input = br#"# keep this comment
mixed-port: 7890
rule-providers:
  existing:
    type: file
    behavior: domain
    path: ./existing.yaml
rules:
  # preserve rule comment
  - MATCH,Proxy
"#;
        let (patched, target) = patch(input, "mihomo-primary", "./rules/nonproxy.yaml", None)
            .unwrap_or_else(|error| panic!("Mihomo 配置接入失败: {error}"));
        let text = String::from_utf8_lossy(&patched);
        assert_eq!(target, "DIRECT");
        assert!(text.contains("# keep this comment"));
        assert!(text.contains("# preserve rule comment"));
        assert!(text.contains("  nonproxy-mihomo-primary:"));
        assert!(text.contains("  - RULE-SET,nonproxy-mihomo-primary,DIRECT"));
        assert_eq!(
            inspect(&patched, "mihomo-primary", "./rules/nonproxy.yaml", None),
            Ok((true, "DIRECT".to_owned()))
        );
        assert_eq!(
            patch(&patched, "mihomo-primary", "./rules/nonproxy.yaml", None).map(|value| value.0),
            Ok(patched)
        );
    }

    #[test]
    fn handles_empty_flow_containers_without_reformatting_other_content() {
        let input = b"mixed-port: 7890\r\nrule-providers: {}\r\nrules: []\r\n";
        let (patched, _) = patch(input, "mihomo-primary", "./nonproxy.yaml", None)
            .unwrap_or_else(|error| panic!("空容器接入失败: {error}"));
        let text = String::from_utf8_lossy(&patched);
        assert!(text.starts_with("mixed-port: 7890\r\n"));
        assert!(text.contains("rule-providers:\r\n"));
        assert!(text.contains("rules:\r\n  - RULE-SET"));
    }

    #[test]
    fn refuses_to_overwrite_a_conflicting_owned_name() {
        let input = br#"rule-providers:
  nonproxy-mihomo-primary:
    type: http
    behavior: classical
    url: https://example.invalid/rules
rules: []
"#;
        assert_eq!(
            patch(input, "mihomo-primary", "./nonproxy.yaml", None),
            Err(AdapterIntegrationError::IntegrationConflict)
        );
    }

    #[test]
    fn refuses_duplicate_critical_top_level_keys() {
        let input = b"rules: []\nrules: []\n";
        assert_eq!(
            patch(input, "mihomo-primary", "./nonproxy.yaml", None),
            Err(AdapterIntegrationError::IntegrationConflict)
        );
    }

    #[test]
    fn refuses_noncanonical_or_duplicate_semantic_critical_keys() {
        let quoted = b"\"rules\": []\n";
        assert_eq!(
            patch(quoted, "mihomo-primary", "./nonproxy.yaml", None),
            Err(AdapterIntegrationError::IntegrationConflict)
        );

        let duplicate = b"rules: []\n!!str rules: []\n";
        assert_eq!(
            patch(duplicate, "mihomo-primary", "./nonproxy.yaml", None),
            Err(AdapterIntegrationError::IntegrationConflict)
        );
    }
}
