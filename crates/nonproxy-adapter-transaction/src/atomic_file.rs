use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use sha2::{Digest, Sha256};

use crate::{AdapterTransactionError, path_guard::validate_managed_file};

const MAXIMUM_MANAGED_RULE_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) fn read_optional_bounded(
    path: &Path,
) -> Result<Option<Vec<u8>>, AdapterTransactionError> {
    validate_managed_file(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AdapterTransactionError::FileTransaction),
    };
    let metadata = file
        .metadata()
        .map_err(|_| AdapterTransactionError::FileTransaction)?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_MANAGED_RULE_BYTES {
        return Err(AdapterTransactionError::ManagedPathInvalid);
    }
    let capacity =
        usize::try_from(metadata.len()).map_err(|_| AdapterTransactionError::ManagedPathInvalid)?;
    let mut bytes = Vec::with_capacity(capacity);
    (&mut file)
        .take(MAXIMUM_MANAGED_RULE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AdapterTransactionError::FileTransaction)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAXIMUM_MANAGED_RULE_BYTES) {
        return Err(AdapterTransactionError::ManagedPathInvalid);
    }
    Ok(Some(bytes))
}

pub(crate) fn write_private_new(path: &Path, bytes: &[u8]) -> Result<(), AdapterTransactionError> {
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAXIMUM_MANAGED_RULE_BYTES) {
        return Err(AdapterTransactionError::ManagedPathInvalid);
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            AdapterTransactionError::ChangeConflict
        } else {
            AdapterTransactionError::FileTransaction
        }
    })?;
    if let Err(error) = write_and_sync(&mut file, bytes) {
        drop(file);
        let _cleanup = fs::remove_file(path);
        return Err(error);
    }
    drop(file);
    let parent = path
        .parent()
        .ok_or(AdapterTransactionError::StateDirectoryInvalid)?;
    sync_directory(parent)
}

pub(crate) fn replace_atomically(
    path: &Path,
    bytes: &[u8],
    change_id: &str,
) -> Result<(), AdapterTransactionError> {
    validate_managed_file(path)?;
    let parent = path
        .parent()
        .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
    let temporary = parent.join(format!(".{file_name}.nonproxy-{change_id}.tmp"));
    write_private_new(&temporary, bytes)?;
    if let Err(_error) = fs::rename(&temporary, path) {
        let _cleanup = fs::remove_file(&temporary);
        return Err(AdapterTransactionError::FileTransaction);
    }
    sync_directory(parent)
}

pub(crate) fn remove_managed_file(path: &Path) -> Result<(), AdapterTransactionError> {
    validate_managed_file(path)?;
    match fs::remove_file(path) {
        Ok(()) => {
            let parent = path
                .parent()
                .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
            sync_directory(parent)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AdapterTransactionError::FileTransaction),
    }
}

pub(crate) fn remove_private_file(path: &Path) -> Result<(), AdapterTransactionError> {
    match fs::remove_file(path) {
        Ok(()) => {
            let parent = path
                .parent()
                .ok_or(AdapterTransactionError::StateDirectoryInvalid)?;
            sync_directory(parent)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AdapterTransactionError::FileTransaction),
    }
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn write_and_sync(file: &mut File, bytes: &[u8]) -> Result<(), AdapterTransactionError> {
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| AdapterTransactionError::FileTransaction)
}

fn sync_directory(path: &Path) -> Result<(), AdapterTransactionError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| AdapterTransactionError::FileTransaction)
}
