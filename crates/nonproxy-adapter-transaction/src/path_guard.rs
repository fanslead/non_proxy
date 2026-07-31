use std::{fs, path::Path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::AdapterTransactionError;

pub(crate) fn prepare_private_state_directory(path: &Path) -> Result<(), AdapterTransactionError> {
    if !path.is_absolute() {
        return Err(AdapterTransactionError::StateDirectoryInvalid);
    }
    reject_symlink(path, AdapterTransactionError::StateDirectoryInvalid)?;
    fs::create_dir_all(path).map_err(|_| AdapterTransactionError::FileTransaction)?;
    reject_symlink(path, AdapterTransactionError::StateDirectoryInvalid)?;
    let metadata = fs::metadata(path).map_err(|_| AdapterTransactionError::FileTransaction)?;
    if !metadata.is_dir() {
        return Err(AdapterTransactionError::StateDirectoryInvalid);
    }
    restrict_directory(path)?;
    for child in ["candidates", "backups", "changes"] {
        let directory = path.join(child);
        reject_symlink(&directory, AdapterTransactionError::StateDirectoryInvalid)?;
        fs::create_dir_all(&directory).map_err(|_| AdapterTransactionError::FileTransaction)?;
        reject_symlink(&directory, AdapterTransactionError::StateDirectoryInvalid)?;
        if !fs::metadata(&directory)
            .map_err(|_| AdapterTransactionError::FileTransaction)?
            .is_dir()
        {
            return Err(AdapterTransactionError::StateDirectoryInvalid);
        }
        restrict_directory(&directory)?;
    }
    Ok(())
}

pub(crate) fn validate_installation_path(
    path: &Path,
) -> Result<std::path::PathBuf, AdapterTransactionError> {
    validate_target_path(path, true)
}

pub(crate) fn validate_main_configuration_path(
    path: &Path,
) -> Result<std::path::PathBuf, AdapterTransactionError> {
    validate_target_path(path, false)
}

fn validate_target_path(
    path: &Path,
    reject_rule_delimiter: bool,
) -> Result<std::path::PathBuf, AdapterTransactionError> {
    let path_text = path
        .to_str()
        .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
    if !path.is_absolute()
        || path.file_name().is_none()
        || path_text
            .chars()
            .any(|character| character.is_control() || (reject_rule_delimiter && character == ','))
    {
        return Err(AdapterTransactionError::ManagedPathInvalid);
    }
    let parent = path
        .parent()
        .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
    reject_symlink(parent, AdapterTransactionError::ManagedPathInvalid)?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|_| AdapterTransactionError::ManagedPathInvalid)?;
    let file_name = path
        .file_name()
        .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
    let canonical_path = canonical_parent.join(file_name);
    validate_managed_file(&canonical_path)?;
    Ok(canonical_path)
}

pub(crate) fn validate_managed_file(path: &Path) -> Result<(), AdapterTransactionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(AdapterTransactionError::ManagedPathInvalid)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AdapterTransactionError::FileTransaction),
    }
}

fn reject_symlink(
    path: &Path,
    error: AdapterTransactionError,
) -> Result<(), AdapterTransactionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(error),
        Ok(_) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AdapterTransactionError::FileTransaction),
    }
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), AdapterTransactionError> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| AdapterTransactionError::FileTransaction)
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), AdapterTransactionError> {
    Ok(())
}
