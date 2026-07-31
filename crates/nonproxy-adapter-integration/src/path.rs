use std::path::{Component, Path};

use crate::AdapterIntegrationError;

pub(crate) fn managed_reference(
    configuration_path: &Path,
    managed_rules_path: &Path,
) -> Result<String, AdapterIntegrationError> {
    if !configuration_path.is_absolute()
        || !managed_rules_path.is_absolute()
        || configuration_path.file_name().is_none()
        || managed_rules_path.file_name().is_none()
    {
        return Err(AdapterIntegrationError::ManagedPathInvalid);
    }
    let configuration_directory = configuration_path
        .parent()
        .ok_or(AdapterIntegrationError::ManagedPathInvalid)?;
    let relative = managed_rules_path
        .strip_prefix(configuration_directory)
        .map_err(|_| AdapterIntegrationError::ManagedPathOutsideConfiguration)?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AdapterIntegrationError::ManagedPathInvalid);
    }
    let value = relative
        .to_str()
        .ok_or(AdapterIntegrationError::ManagedPathInvalid)?;
    if value.len() > 2_048
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        return Err(AdapterIntegrationError::ManagedPathInvalid);
    }
    Ok(format!("./{value}"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{AdapterIntegrationError, path::managed_reference};

    #[test]
    fn only_allows_a_relative_child_reference() {
        assert_eq!(
            managed_reference(
                Path::new("/client/config.yaml"),
                Path::new("/client/rules/nonproxy.yaml")
            ),
            Ok("./rules/nonproxy.yaml".to_owned())
        );
        assert_eq!(
            managed_reference(
                Path::new("/client/config.yaml"),
                Path::new("/other/nonproxy.yaml")
            ),
            Err(AdapterIntegrationError::ManagedPathOutsideConfiguration)
        );
        assert_eq!(
            managed_reference(
                Path::new("/client/config.yaml"),
                Path::new("/client/rules/nonproxy#override.yaml")
            ),
            Err(AdapterIntegrationError::ManagedPathInvalid)
        );
    }
}
