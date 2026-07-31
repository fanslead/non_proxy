use std::collections::BTreeSet;

use jsonc_parser::{
    ParseOptions,
    cst::{CstInputValue, CstNode, CstObject, CstRootNode},
};
use serde_json::{Map, Value};

use crate::{AdapterIntegrationError, model::reference_name};

pub(crate) fn patch(
    configuration: &[u8],
    integration_id: &str,
    managed_reference: &str,
    direct_target: Option<&str>,
) -> Result<(Vec<u8>, String), AdapterIntegrationError> {
    let input = std::str::from_utf8(configuration)
        .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)?;
    let root = parse(input)?;
    reject_duplicate_keys(&root)?;
    let semantic = root
        .to_serde_value()
        .ok_or(AdapterIntegrationError::ConfigurationInvalid)?;
    let target = resolve_direct_target(&semantic, direct_target)?;
    let name = reference_name(integration_id);
    inspect_semantic(&semantic, &name, managed_reference, &target)?;

    let root_object = root
        .object_value()
        .ok_or(AdapterIntegrationError::ConfigurationInvalid)?;
    let route = optional_or_create_object(&root_object, "route")?;
    let semantic_route = semantic.get("route").and_then(Value::as_object);
    let has_rule_set = semantic_route
        .and_then(|value| value.get("rule_set"))
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| rule_set_name(value) == Some(&name))
        });
    if !has_rule_set {
        let rule_sets = optional_or_create_array(&route, "rule_set")?;
        rule_sets.insert(
            0,
            object_value([
                ("type", "local"),
                ("tag", name.as_str()),
                ("format", "source"),
                ("path", managed_reference),
            ]),
        );
    }
    let has_route_rule = semantic_route
        .and_then(|value| value.get("rules"))
        .and_then(Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| route_rule_matches(value, &name, &target))
        });
    if !has_route_rule {
        let rules = optional_or_create_array(&route, "rules")?;
        rules.insert(
            0,
            object_value([
                ("rule_set", name.as_str()),
                ("action", "route"),
                ("outbound", target.as_str()),
            ]),
        );
    }
    let output = root.to_string().into_bytes();
    let (integrated, inspected_target) =
        inspect(&output, integration_id, managed_reference, Some(&target))?;
    if !integrated {
        return Err(AdapterIntegrationError::ConfigurationInvalid);
    }
    Ok((output, inspected_target))
}

pub(crate) fn inspect(
    configuration: &[u8],
    integration_id: &str,
    managed_reference: &str,
    direct_target: Option<&str>,
) -> Result<(bool, String), AdapterIntegrationError> {
    let input = std::str::from_utf8(configuration)
        .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)?;
    let root = parse(input)?;
    reject_duplicate_keys(&root)?;
    let semantic = root
        .to_serde_value()
        .ok_or(AdapterIntegrationError::ConfigurationInvalid)?;
    let target = resolve_direct_target(&semantic, direct_target)?;
    let name = reference_name(integration_id);
    let integrated = inspect_semantic(&semantic, &name, managed_reference, &target)?;
    Ok((integrated, target))
}

fn parse(input: &str) -> Result<CstRootNode, AdapterIntegrationError> {
    CstRootNode::parse(
        input,
        &ParseOptions {
            allow_comments: true,
            allow_loose_object_property_names: false,
            allow_trailing_commas: true,
            allow_missing_commas: false,
            allow_single_quoted_strings: false,
            allow_hexadecimal_numbers: false,
            allow_unary_plus_numbers: false,
        },
    )
    .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)
}

fn reject_duplicate_keys(root: &CstRootNode) -> Result<(), AdapterIntegrationError> {
    let object = root
        .object_value()
        .ok_or(AdapterIntegrationError::ConfigurationInvalid)?;
    reject_object_duplicate_keys(&object)
}

fn reject_object_duplicate_keys(object: &CstObject) -> Result<(), AdapterIntegrationError> {
    let mut names = BTreeSet::new();
    for property in object.properties() {
        let name = property
            .name()
            .ok_or(AdapterIntegrationError::ConfigurationInvalid)?
            .decoded_value()
            .map_err(|_| AdapterIntegrationError::ConfigurationInvalid)?;
        if !names.insert(name) {
            return Err(AdapterIntegrationError::IntegrationConflict);
        }
        if let Some(value) = property.value() {
            reject_node_duplicate_keys(&value)?;
        }
    }
    Ok(())
}

fn reject_node_duplicate_keys(value: &CstNode) -> Result<(), AdapterIntegrationError> {
    if let Some(object) = value.as_object() {
        return reject_object_duplicate_keys(&object);
    }
    if let Some(array) = value.as_array() {
        for element in array.elements() {
            reject_node_duplicate_keys(&element)?;
        }
    }
    Ok(())
}

fn resolve_direct_target(
    configuration: &Value,
    requested: Option<&str>,
) -> Result<String, AdapterIntegrationError> {
    let outbounds = configuration
        .get("outbounds")
        .and_then(Value::as_array)
        .ok_or(AdapterIntegrationError::DirectTargetInvalid)?;
    let direct = outbounds
        .iter()
        .filter(|outbound| outbound.get("type").and_then(Value::as_str) == Some("direct"))
        .filter_map(|outbound| outbound.get("tag").and_then(Value::as_str))
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    if let Some(requested) = requested {
        if direct.contains(&requested) {
            return Ok(requested.to_owned());
        }
        return Err(AdapterIntegrationError::DirectTargetInvalid);
    }
    match direct.as_slice() {
        [target] => Ok((*target).to_owned()),
        _ => Err(AdapterIntegrationError::DirectTargetAmbiguous),
    }
}

fn inspect_semantic(
    configuration: &Value,
    name: &str,
    managed_reference: &str,
    direct_target: &str,
) -> Result<bool, AdapterIntegrationError> {
    let Some(route) = configuration.get("route") else {
        return Ok(false);
    };
    let route = route
        .as_object()
        .ok_or(AdapterIntegrationError::IntegrationConflict)?;
    let rule_set_entries = optional_array(route, "rule_set")?;
    let matching_rule_sets = rule_set_entries
        .iter()
        .filter(|value| rule_set_name(value) == Some(name))
        .collect::<Vec<_>>();
    let rule_set_present = match matching_rule_sets.as_slice() {
        [] => false,
        [value] if rule_set_matches(value, name, managed_reference) => true,
        _ => return Err(AdapterIntegrationError::IntegrationConflict),
    };
    let route_rules = optional_array(route, "rules")?;
    let matching_routes = route_rules
        .iter()
        .filter(|value| route_rule_references(value, name))
        .collect::<Vec<_>>();
    let route_present = match matching_routes.as_slice() {
        [] => false,
        [value] if route_rule_matches(value, name, direct_target) => true,
        _ => return Err(AdapterIntegrationError::IntegrationConflict),
    };
    Ok(rule_set_present && route_present)
}

fn optional_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a [Value], AdapterIntegrationError> {
    match object.get(key) {
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or(AdapterIntegrationError::IntegrationConflict),
        None => Ok(&[]),
    }
}

fn rule_set_name(value: &Value) -> Option<&str> {
    value.get("tag").and_then(Value::as_str)
}

fn rule_set_matches(value: &Value, name: &str, managed_reference: &str) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 4
            && object.get("type").and_then(Value::as_str) == Some("local")
            && object.get("tag").and_then(Value::as_str) == Some(name)
            && object.get("format").and_then(Value::as_str) == Some("source")
            && object.get("path").and_then(Value::as_str) == Some(managed_reference)
    })
}

fn route_rule_references(value: &Value, name: &str) -> bool {
    let Some(rule_set) = value.get("rule_set") else {
        return false;
    };
    match rule_set {
        Value::String(value) => value == name,
        Value::Array(values) => values.iter().any(|value| value.as_str() == Some(name)),
        _ => false,
    }
}

fn route_rule_matches(value: &Value, name: &str, direct_target: &str) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 3
            && object.get("rule_set").and_then(Value::as_str) == Some(name)
            && object.get("action").and_then(Value::as_str) == Some("route")
            && object.get("outbound").and_then(Value::as_str) == Some(direct_target)
    })
}

fn optional_or_create_object(
    parent: &CstObject,
    name: &str,
) -> Result<CstObject, AdapterIntegrationError> {
    parent
        .object_value_or_create(name)
        .ok_or(AdapterIntegrationError::IntegrationConflict)
}

fn optional_or_create_array(
    parent: &CstObject,
    name: &str,
) -> Result<jsonc_parser::cst::CstArray, AdapterIntegrationError> {
    parent
        .array_value_or_create(name)
        .ok_or(AdapterIntegrationError::IntegrationConflict)
}

fn object_value<const N: usize>(values: [(&str, &str); N]) -> CstInputValue {
    CstInputValue::Object(
        values
            .into_iter()
            .map(|(key, value)| (key.to_owned(), CstInputValue::String(value.to_owned())))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use crate::AdapterIntegrationError;

    use super::{inspect, patch};

    #[test]
    fn preserves_jsonc_comments_and_selects_the_only_direct_outbound() {
        let input = br#"{
  // keep this comment
  "outbounds": [
    { "type": "direct", "tag": "direct-out" },
    { "type": "socks", "tag": "proxy-out", "server": "127.0.0.1", "server_port": 1080 },
  ],
  "route": {
    "rules": [
      // keep route comment
      { "action": "route", "outbound": "proxy-out" },
    ],
  },
}
"#;
        let (patched, target) = patch(input, "sing-primary", "./rules/nonproxy.json", None)
            .unwrap_or_else(|error| panic!("sing-box 配置接入失败: {error}"));
        let text = String::from_utf8_lossy(&patched);
        assert_eq!(target, "direct-out");
        assert!(text.contains("// keep this comment"));
        assert!(text.contains("// keep route comment"));
        assert!(text.contains("\"tag\": \"nonproxy-sing-primary\""));
        assert!(text.contains("\"outbound\": \"direct-out\""));
        assert_eq!(
            inspect(&patched, "sing-primary", "./rules/nonproxy.json", None),
            Ok((true, "direct-out".to_owned()))
        );
        assert_eq!(
            patch(&patched, "sing-primary", "./rules/nonproxy.json", None).map(|value| value.0),
            Ok(patched)
        );
    }

    #[test]
    fn requires_an_explicit_target_when_multiple_direct_outbounds_exist() {
        let input = br#"{
          "outbounds": [
            {"type":"direct","tag":"direct-a"},
            {"type":"direct","tag":"direct-b"}
          ]
        }"#;
        assert_eq!(
            patch(input, "sing-primary", "./nonproxy.json", None),
            Err(AdapterIntegrationError::DirectTargetAmbiguous)
        );
        assert!(patch(input, "sing-primary", "./nonproxy.json", Some("direct-b")).is_ok());
    }

    #[test]
    fn refuses_duplicate_jsonc_keys_before_semantic_projection() {
        let input = br#"{
          "outbounds": [{"type":"direct","tag":"direct-out"}],
          "route": {},
          "route": {}
        }"#;
        assert_eq!(
            patch(input, "sing-primary", "./nonproxy.json", None),
            Err(AdapterIntegrationError::IntegrationConflict)
        );
    }
}
