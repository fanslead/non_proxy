use std::{fs::OpenOptions, io::Read, path::Path};

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::AdapterHostError;

pub(crate) const MAXIMUM_CONFIGURATION_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) fn read_bounded_regular(path: &Path) -> Result<Vec<u8>, AdapterHostError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options
        .open(path)
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_CONFIGURATION_BYTES {
        return Err(AdapterHostError::ClientControlUnavailable);
    }
    let mut bytes = Vec::new();
    file.take(MAXIMUM_CONFIGURATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAXIMUM_CONFIGURATION_BYTES) {
        return Err(AdapterHostError::ClientControlUnavailable);
    }
    Ok(bytes)
}

pub(crate) fn sha256_bounded_regular(path: &Path) -> Result<[u8; 32], AdapterHostError> {
    let bytes = Zeroizing::new(read_bounded_regular(path)?);
    Ok(Sha256::digest(&bytes).into())
}
