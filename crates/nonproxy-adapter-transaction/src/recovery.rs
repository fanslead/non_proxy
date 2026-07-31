use std::{collections::BTreeSet, fs, path::Path};

use crate::{
    AdapterTransactionError,
    atomic_file::{read_optional_bounded, remove_private_file, sha256},
    digest::decode_hash,
    identifier::validate_identifier,
    manifest::ChangeManifest,
};

pub(crate) fn recover_state(state_directory: &Path) -> Result<(), AdapterTransactionError> {
    let mut candidates = BTreeSet::new();
    let mut backups = BTreeSet::new();
    for entry in fs::read_dir(state_directory.join("changes"))
        .map_err(|_| AdapterTransactionError::FileTransaction)?
    {
        let entry = entry.map_err(|_| AdapterTransactionError::FileTransaction)?;
        if !entry
            .file_type()
            .map_err(|_| AdapterTransactionError::FileTransaction)?
            .is_file()
        {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        let manifest = ChangeManifest::read(&entry.path())?;
        validate_identifier(&manifest.change_id)
            .map_err(|_| AdapterTransactionError::StateCorrupt)?;
        validate_identifier(&manifest.backup_id)
            .map_err(|_| AdapterTransactionError::StateCorrupt)?;
        let expected_manifest_name = format!("{}.json", manifest.change_id);
        if entry.file_name() != expected_manifest_name.as_str() {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        let candidate_hash = decode_hash(&manifest.candidate_sha256)?;
        let candidate_name = format!("{}.rules", manifest.change_id);
        let candidate =
            read_optional_bounded(&state_directory.join("candidates").join(&candidate_name))?
                .ok_or(AdapterTransactionError::StateCorrupt)?;
        if sha256(&candidate) != candidate_hash || !candidates.insert(candidate_name) {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        match (manifest.backup_existed, manifest.backup_sha256.as_deref()) {
            (true, Some(expected)) => {
                let backup_name = format!("{}.rules", manifest.backup_id);
                let backup =
                    read_optional_bounded(&state_directory.join("backups").join(&backup_name))?
                        .ok_or(AdapterTransactionError::StateCorrupt)?;
                if sha256(&backup) != decode_hash(expected)? || !backups.insert(backup_name) {
                    return Err(AdapterTransactionError::StateCorrupt);
                }
            }
            (false, None) => {}
            _ => return Err(AdapterTransactionError::StateCorrupt),
        }
    }
    remove_orphans(state_directory, "candidates", &candidates)?;
    remove_orphans(state_directory, "backups", &backups)
}

fn remove_orphans(
    state_directory: &Path,
    directory: &str,
    retained: &BTreeSet<String>,
) -> Result<(), AdapterTransactionError> {
    for entry in fs::read_dir(state_directory.join(directory))
        .map_err(|_| AdapterTransactionError::FileTransaction)?
    {
        let entry = entry.map_err(|_| AdapterTransactionError::FileTransaction)?;
        if !entry
            .file_type()
            .map_err(|_| AdapterTransactionError::FileTransaction)?
            .is_file()
        {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| AdapterTransactionError::StateCorrupt)?;
        if !retained.contains(&name) {
            remove_private_file(&entry.path())?;
        }
    }
    Ok(())
}
