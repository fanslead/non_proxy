use crate::AdapterIntegrationError;

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
    let line_ending = if input.contains("\r\n") { "\r\n" } else { "\n" };
    let begin = begin_marker(integration_id);
    let end = end_marker(integration_id);
    let managed_line = format!("RULE-SET,{managed_reference},{DIRECT_TARGET}");
    let block = format!("{begin}{line_ending}{managed_line}{line_ending}{end}{line_ending}");
    let bounds = owned_block_bounds(input, &begin, &end)?;
    let output = if let Some((start, finish)) = bounds {
        if !offset_inside_rule_section(input, start) || !is_first_effective_rule(input, start) {
            return Err(AdapterIntegrationError::IntegrationConflict);
        }
        let mut value = String::with_capacity(input.len() + block.len());
        value.push_str(&input[..start]);
        value.push_str(&block);
        value.push_str(&input[finish..]);
        value
    } else if let Some(offset) = rule_section_content_offset(input) {
        let separator = if offset > 0 && !input[..offset].ends_with('\n') {
            line_ending
        } else {
            ""
        };
        let mut value = String::with_capacity(input.len() + block.len() + separator.len());
        value.push_str(&input[..offset]);
        value.push_str(separator);
        value.push_str(&block);
        value.push_str(&input[offset..]);
        value
    } else {
        let mut value = input.to_owned();
        if !value.is_empty() && !value.ends_with('\n') {
            value.push_str(line_ending);
        }
        if !value.is_empty() {
            value.push_str(line_ending);
        }
        value.push_str("[Rule]");
        value.push_str(line_ending);
        value.push_str(&block);
        value
    };
    let (integrated, _) = inspect(
        output.as_bytes(),
        integration_id,
        managed_reference,
        direct_target,
    )?;
    if !integrated {
        return Err(AdapterIntegrationError::ConfigurationInvalid);
    }
    Ok((output.into_bytes(), DIRECT_TARGET.to_owned()))
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
    let begin = begin_marker(integration_id);
    let end = end_marker(integration_id);
    let Some((start, finish)) = owned_block_bounds(input, &begin, &end)? else {
        return Ok((false, DIRECT_TARGET.to_owned()));
    };
    if !offset_inside_rule_section(input, start) {
        return Err(AdapterIntegrationError::IntegrationConflict);
    }
    if !is_first_effective_rule(input, start) {
        return Err(AdapterIntegrationError::IntegrationConflict);
    }
    let expected = format!("RULE-SET,{managed_reference},{DIRECT_TARGET}");
    let content = &input[start..finish];
    let effective = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    Ok((effective == [expected.as_str()], DIRECT_TARGET.to_owned()))
}

fn validate_direct_target(value: Option<&str>) -> Result<(), AdapterIntegrationError> {
    if value.is_some_and(|value| !value.eq_ignore_ascii_case(DIRECT_TARGET)) {
        return Err(AdapterIntegrationError::DirectTargetInvalid);
    }
    Ok(())
}

fn begin_marker(integration_id: &str) -> String {
    format!("# >>> NonProxy managed route: {integration_id}")
}

fn end_marker(integration_id: &str) -> String {
    format!("# <<< NonProxy managed route: {integration_id}")
}

fn owned_block_bounds(
    input: &str,
    begin: &str,
    end: &str,
) -> Result<Option<(usize, usize)>, AdapterIntegrationError> {
    let begins = line_offsets(input)
        .filter(|(_, line)| line.trim_end_matches(['\r', '\n']) == begin)
        .collect::<Vec<_>>();
    let ends = line_offsets(input)
        .filter(|(_, line)| line.trim_end_matches(['\r', '\n']) == end)
        .collect::<Vec<_>>();
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([(begin_offset, _)], [(end_offset, end_line)]) if begin_offset < end_offset => {
            Ok(Some((*begin_offset, end_offset + end_line.len())))
        }
        _ => Err(AdapterIntegrationError::IntegrationConflict),
    }
}

fn line_offsets(input: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0_usize;
    input.split_inclusive('\n').map(move |line| {
        let current = offset;
        offset += line.len();
        (current, line)
    })
}

fn rule_section_content_offset(input: &str) -> Option<usize> {
    line_offsets(input).find_map(|(offset, line)| {
        (line.trim().eq_ignore_ascii_case("[Rule]")).then_some(offset + line.len())
    })
}

fn offset_inside_rule_section(input: &str, target: usize) -> bool {
    let mut current_section = None;
    for (offset, line) in line_offsets(input) {
        if offset > target {
            break;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = Some(trimmed);
        }
    }
    current_section.is_some_and(|section| section.eq_ignore_ascii_case("[Rule]"))
}

fn is_first_effective_rule(input: &str, target: usize) -> bool {
    let Some(section_content) = rule_section_content_offset(input) else {
        return false;
    };
    if section_content > target {
        return false;
    }
    input[section_content..target]
        .lines()
        .map(str::trim)
        .all(|line| line.is_empty() || line.starts_with('#'))
}

#[cfg(test)]
mod tests {
    use super::{inspect, patch};

    #[test]
    fn inserts_owned_rule_first_and_preserves_crlf() {
        let input = b"[General]\r\nloglevel = notify\r\n\r\n[Rule]\r\nFINAL,Proxy\r\n";
        let (patched, target) = patch(input, "surge-primary", "./nonproxy.list", None)
            .unwrap_or_else(|error| panic!("Surge 配置接入失败: {error}"));
        let text = String::from_utf8_lossy(&patched);
        assert_eq!(target, "DIRECT");
        assert!(text.contains(
            "[Rule]\r\n# >>> NonProxy managed route: surge-primary\r\nRULE-SET,./nonproxy.list,DIRECT\r\n"
        ));
        assert_eq!(
            inspect(&patched, "surge-primary", "./nonproxy.list", Some("direct")),
            Ok((true, "DIRECT".to_owned()))
        );
        assert_eq!(
            patch(&patched, "surge-primary", "./nonproxy.list", None).map(|value| value.0),
            Ok(patched)
        );
    }

    #[test]
    fn separates_a_rule_section_without_a_trailing_newline() {
        let (patched, _) = patch(b"[Rule]", "surge-primary", "./nonproxy.list", None)
            .unwrap_or_else(|error| panic!("Surge 配置接入失败: {error}"));
        assert!(patched.starts_with(b"[Rule]\n# >>> NonProxy"));
        assert_eq!(
            inspect(&patched, "surge-primary", "./nonproxy.list", None),
            Ok((true, "DIRECT".to_owned()))
        );
    }

    #[test]
    fn refuses_an_owned_block_after_an_effective_rule() {
        let input = b"[Rule]\nDOMAIN,example.com,Proxy\n# >>> NonProxy managed route: surge-primary\nRULE-SET,./nonproxy.list,DIRECT\n# <<< NonProxy managed route: surge-primary\n";
        assert_eq!(
            patch(input, "surge-primary", "./nonproxy.list", None),
            Err(crate::AdapterIntegrationError::IntegrationConflict)
        );
    }
}
