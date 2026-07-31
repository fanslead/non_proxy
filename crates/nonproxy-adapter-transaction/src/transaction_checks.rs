use crate::{
    AdapterTransactionError, atomic_file::sha256, digest::decode_hash,
    identifier::validate_identifier, manifest::ChangeManifest, types::AdapterInstallation,
};

pub(crate) fn validate_installation(
    installation: &AdapterInstallation,
) -> Result<(), AdapterTransactionError> {
    validate_identifier(&installation.adapter_id)
        .map_err(|_| AdapterTransactionError::InstallationInvalid)
}

pub(crate) fn matches_backup(
    manifest: &ChangeManifest,
    current: Option<&[u8]>,
) -> Result<bool, AdapterTransactionError> {
    match (manifest.backup_existed, &manifest.backup_sha256, current) {
        (false, None, None) => Ok(true),
        (true, Some(expected), Some(bytes)) => Ok(sha256(bytes) == decode_hash(expected)?),
        (false, None, Some(_)) | (true, Some(_), None) => Ok(false),
        _ => Err(AdapterTransactionError::StateCorrupt),
    }
}
