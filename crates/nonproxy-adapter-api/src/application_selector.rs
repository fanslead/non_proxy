use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::{AdapterContractError, model::POLICY_FORMAT_VERSION};

const APPLICATION_SELECTOR_VERSION: u32 = 1;
const MAXIMUM_PATH_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationSelectorPlatform {
    Macos,
    Windows,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationPathKind {
    Bundle,
    Executable,
    PackageFamily,
}

pub(crate) fn normalize_application_selector(
    format_version: u32,
    selector_version: u32,
    platform: ApplicationSelectorPlatform,
    path_kind: ApplicationPathKind,
    value: &mut String,
) -> Result<(), AdapterContractError> {
    if format_version != POLICY_FORMAT_VERSION || selector_version != APPLICATION_SELECTOR_VERSION {
        return Err(AdapterContractError::SelectorInvalid);
    }
    match (platform, path_kind) {
        (ApplicationSelectorPlatform::Macos, ApplicationPathKind::Bundle) => {
            normalize_macos_bundle(value)
        }
        (ApplicationSelectorPlatform::Windows, ApplicationPathKind::Executable) => {
            validate_windows_executable(value)
        }
        (ApplicationSelectorPlatform::Windows, ApplicationPathKind::PackageFamily) => {
            validate_windows_package_family(value)
        }
        _ => Err(AdapterContractError::SelectorInvalid),
    }
}

fn normalize_macos_bundle(value: &mut String) -> Result<(), AdapterContractError> {
    let trimmed = value.trim_end_matches('/');
    let path = Path::new(trimmed);
    if trimmed.is_empty()
        || trimmed.len() > MAXIMUM_PATH_BYTES
        || !trimmed.starts_with('/')
        || !trimmed.ends_with(".app")
        || trimmed.contains('\\')
        || trimmed
            .split('/')
            .any(|component| matches!(component, "." | ".."))
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        || has_unsafe_path_character(trimmed)
    {
        return Err(AdapterContractError::SelectorInvalid);
    }
    *value = format!("{trimmed}/");
    Ok(())
}

fn validate_windows_executable(value: &str) -> Result<(), AdapterContractError> {
    let bytes = value.as_bytes();
    if value.is_empty()
        || value.len() > MAXIMUM_PATH_BYTES
        || bytes.len() < 7
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || value.contains('/')
        || value[3..].split('\\').any(|segment| {
            segment.is_empty() || matches!(segment, "." | "..") || segment.ends_with([' ', '.'])
        })
        || !value.to_ascii_lowercase().ends_with(".exe")
        || value[2..].contains(':')
        || value.contains(['*', '?'])
        || has_unsafe_path_character(value)
    {
        return Err(AdapterContractError::SelectorInvalid);
    }
    Ok(())
}

fn validate_windows_package_family(value: &str) -> Result<(), AdapterContractError> {
    let Some((name, publisher)) = value.rsplit_once('_') else {
        return Err(AdapterContractError::SelectorInvalid);
    };
    if value.len() > 255
        || name.is_empty()
        || publisher.len() != 13
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        || !publisher.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(AdapterContractError::SelectorInvalid);
    }
    Ok(())
}

fn has_unsafe_path_character(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() || matches!(character, ',' | '\0'))
}
