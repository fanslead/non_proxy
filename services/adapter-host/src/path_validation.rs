use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::AdapterHostError;

const MAXIMUM_PATH_BYTES: usize = 4_096;

pub(crate) fn canonical_executable(path: &Path) -> Result<PathBuf, AdapterHostError> {
    validate_path_shape(path)?;
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
    validate_path_shape(path)?;
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

fn validate_path_shape(path: &Path) -> Result<(), AdapterHostError> {
    let value = path.to_string_lossy();
    if !path.is_absolute()
        || path.file_name().is_none()
        || value.len() > MAXIMUM_PATH_BYTES
        || value.chars().any(|character| character.is_control())
    {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(())
}
