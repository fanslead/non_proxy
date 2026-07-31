use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::AdapterHostError;

const MAXIMUM_PATH_BYTES: usize = 4_096;

pub(crate) fn canonical_executable(path: &Path) -> Result<PathBuf, AdapterHostError> {
    validate_path_shape(path, false)?;
    let canonical = path
        .canonicalize()
        .map_err(|_| AdapterHostError::InstallationInvalid)?;
    let metadata = fs::metadata(&canonical).map_err(|_| AdapterHostError::InstallationInvalid)?;
    if !metadata.is_file() {
        return Err(AdapterHostError::InstallationInvalid);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(canonical)
}

pub(crate) fn canonical_managed_path(path: &Path) -> Result<PathBuf, AdapterHostError> {
    validate_path_shape(path, true)?;
    let parent = path.parent().ok_or(AdapterHostError::InstallationInvalid)?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| AdapterHostError::InstallationInvalid)?;
    let file_name = path
        .file_name()
        .ok_or(AdapterHostError::InstallationInvalid)?;
    let canonical = canonical_parent.join(file_name);
    match fs::symlink_metadata(&canonical) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(AdapterHostError::InstallationInvalid)
        }
        Ok(_) => Ok(canonical),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(canonical),
        Err(error) => Err(AdapterHostError::File(error)),
    }
}

pub(crate) fn canonical_main_configuration(path: &Path) -> Result<PathBuf, AdapterHostError> {
    validate_path_shape(path, false)?;
    let selected_metadata =
        fs::symlink_metadata(path).map_err(|_| AdapterHostError::InstallationInvalid)?;
    if selected_metadata.file_type().is_symlink() || !selected_metadata.is_file() {
        return Err(AdapterHostError::InstallationInvalid);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| AdapterHostError::InstallationInvalid)?;
    let metadata =
        fs::symlink_metadata(&canonical).map_err(|_| AdapterHostError::InstallationInvalid)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(canonical)
}

pub(crate) fn validate_integration_paths(
    configuration: &Path,
    managed: &Path,
) -> Result<(), AdapterHostError> {
    let parent = configuration
        .parent()
        .ok_or(AdapterHostError::InstallationInvalid)?;
    if configuration == managed || managed.strip_prefix(parent).is_err() {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(())
}

fn validate_path_shape(path: &Path, reject_rule_delimiter: bool) -> Result<(), AdapterHostError> {
    let value = path.to_str().ok_or(AdapterHostError::InstallationInvalid)?;
    if !path.is_absolute()
        || path.file_name().is_none()
        || value.len() > MAXIMUM_PATH_BYTES
        || value
            .chars()
            .any(|character| character.is_control() || (reject_rule_delimiter && character == ','))
    {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{canonical_main_configuration, canonical_managed_path, validate_integration_paths};

    #[cfg(unix)]
    #[test]
    fn main_configuration_symlinks_and_outside_sidecars_are_rejected() {
        use std::os::unix::fs::symlink;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let configuration_directory = directory.path().join("configuration");
        let outside_directory = directory.path().join("outside");
        fs::create_dir(&configuration_directory)
            .and_then(|()| fs::create_dir(&outside_directory))
            .unwrap_or_else(|error| panic!("路径测试目录创建失败: {error}"));
        let configuration = configuration_directory.join("config.yaml");
        let alias = configuration_directory.join("alias.yaml");
        fs::write(&configuration, "rules: []\n")
            .unwrap_or_else(|error| panic!("主配置写入失败: {error}"));
        symlink(&configuration, &alias)
            .unwrap_or_else(|error| panic!("主配置符号链接创建失败: {error}"));

        assert!(canonical_main_configuration(&alias).is_err());
        let canonical_configuration = canonical_main_configuration(&configuration)
            .unwrap_or_else(|error| panic!("主配置规范化失败: {error}"));
        let outside = canonical_managed_path(&outside_directory.join("nonproxy.yaml"))
            .unwrap_or_else(|error| panic!("外部 sidecar 规范化失败: {error}"));
        assert!(validate_integration_paths(&canonical_configuration, &outside).is_err());
    }
}
