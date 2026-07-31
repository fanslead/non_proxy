use std::path::Path;

use crate::{
    AdapterTransactionError, AdapterTransactionManager,
    atomic_file::{read_optional_bounded, replace_atomically_preserving_permissions, sha256},
    digest::decode_hash,
    integrated::{read_hashed_required, restore_rules_backup},
    manifest::{ChangeManifest, ConfigurationManifest},
    path_guard::{validate_installation_path, validate_main_configuration_path},
    transaction_checks::matches_backup,
    types::{RollbackOutcome, VerificationOutcome},
};

pub(crate) fn verify_integrated_manifest(
    manifest: &ChangeManifest,
) -> Result<VerificationOutcome, AdapterTransactionError> {
    let configuration = manifest
        .configuration
        .as_ref()
        .ok_or(AdapterTransactionError::StateCorrupt)?;
    let rules_hash = decode_hash(&manifest.candidate_sha256)?;
    let configuration_hash = decode_hash(&configuration.candidate_sha256)?;
    let rules_target = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
    let configuration_target = validate_main_configuration_path(Path::new(&configuration.path))?;
    let rules_current = read_optional_bounded(&rules_target)?;
    let configuration_current = read_optional_bounded(&configuration_target)?;
    let configuration_verified = rules_current
        .as_deref()
        .is_some_and(|bytes| sha256(bytes) == rules_hash)
        && configuration_current
            .as_deref()
            .is_some_and(|bytes| sha256(bytes) == configuration_hash);
    Ok(VerificationOutcome {
        configuration_verified,
        path_verified: false,
        candidate_sha256: rules_hash,
        configuration_candidate_sha256: Some(configuration_hash),
    })
}

pub(crate) fn rollback_integrated_locked(
    manager: &AdapterTransactionManager,
    manifest: &ChangeManifest,
) -> Result<RollbackOutcome, AdapterTransactionError> {
    let configuration = manifest
        .configuration
        .as_ref()
        .ok_or(AdapterTransactionError::StateCorrupt)?;
    let rules_target = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
    let configuration_target = validate_main_configuration_path(Path::new(&configuration.path))?;
    let rules_current = read_optional_bounded(&rules_target)?;
    let configuration_current = read_optional_bounded(&configuration_target)?;
    let states = IntegratedTargetStates::new(
        manifest,
        configuration,
        rules_current.as_deref(),
        configuration_current.as_deref(),
    )?;
    states.require_known()?;
    if states.rules.backup && states.configuration.backup {
        return Ok(RollbackOutcome {
            restored: true,
            replayed: true,
        });
    }
    if !states.configuration.backup {
        let current = read_optional_bounded(&configuration_target)?;
        let fresh = configuration_state(configuration, current.as_deref())?;
        fresh.require_known()?;
        if !fresh.backup {
            let backup_hash = decode_hash(&configuration.backup_sha256)?;
            let backup = read_hashed_required(
                &manager.configuration_backup_path(&manifest.backup_id),
                &backup_hash,
            )?;
            replace_atomically_preserving_permissions(
                &configuration_target,
                &backup,
                &manifest.change_id,
            )?;
        }
    }
    if !states.rules.backup {
        let current = read_optional_bounded(&rules_target)?;
        let fresh = rules_state(manifest, current.as_deref())?;
        fresh.require_known()?;
        if !fresh.backup {
            restore_rules_backup(manager, manifest, &rules_target, &manifest.change_id)?;
        }
    }
    if !integrated_targets_are_backups(manifest)? {
        return Err(AdapterTransactionError::FileTransaction);
    }
    Ok(RollbackOutcome {
        restored: true,
        replayed: false,
    })
}

pub(crate) fn integrated_targets_are_backups(
    manifest: &ChangeManifest,
) -> Result<bool, AdapterTransactionError> {
    let configuration = manifest
        .configuration
        .as_ref()
        .ok_or(AdapterTransactionError::StateCorrupt)?;
    let rules_target = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
    let configuration_target = validate_main_configuration_path(Path::new(&configuration.path))?;
    let rules_current = read_optional_bounded(&rules_target)?;
    let configuration_current = read_optional_bounded(&configuration_target)?;
    let configuration_backup_hash = decode_hash(&configuration.backup_sha256)?;
    Ok(matches_backup(manifest, rules_current.as_deref())?
        && configuration_current
            .as_deref()
            .is_some_and(|bytes| sha256(bytes) == configuration_backup_hash))
}

pub(crate) fn validate_manifest_backups(
    manager: &AdapterTransactionManager,
    manifest: &ChangeManifest,
) -> Result<(), AdapterTransactionError> {
    match (manifest.backup_existed, manifest.backup_sha256.as_deref()) {
        (true, Some(expected)) => {
            let expected = decode_hash(expected)?;
            let _backup =
                read_hashed_required(&manager.backup_path(&manifest.backup_id), &expected)?;
        }
        (false, None) => {}
        _ => return Err(AdapterTransactionError::StateCorrupt),
    }
    let configuration = manifest
        .configuration
        .as_ref()
        .ok_or(AdapterTransactionError::StateCorrupt)?;
    let expected = decode_hash(&configuration.backup_sha256)?;
    let _backup = read_hashed_required(
        &manager.configuration_backup_path(&manifest.backup_id),
        &expected,
    )?;
    Ok(())
}

pub(crate) fn recover_partial_integrated(
    manager: &AdapterTransactionManager,
    manifest: &ChangeManifest,
) -> Result<(), AdapterTransactionError> {
    let configuration = manifest
        .configuration
        .as_ref()
        .ok_or(AdapterTransactionError::StateCorrupt)?;
    let rules_target = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
    let configuration_target = validate_main_configuration_path(Path::new(&configuration.path))?;
    let rules_current = read_optional_bounded(&rules_target)?;
    let configuration_current = read_optional_bounded(&configuration_target)?;
    let states = IntegratedTargetStates::new(
        manifest,
        configuration,
        rules_current.as_deref(),
        configuration_current.as_deref(),
    )?;
    if states.rules.candidate
        && states.configuration.backup
        && !states.rules.backup
        && !states.configuration.candidate
    {
        restore_rules_backup(manager, manifest, &rules_target, &manifest.change_id)?;
    } else if states.rules.backup
        && states.configuration.candidate
        && !states.rules.candidate
        && !states.configuration.backup
    {
        let backup_hash = decode_hash(&configuration.backup_sha256)?;
        let backup = read_hashed_required(
            &manager.configuration_backup_path(&manifest.backup_id),
            &backup_hash,
        )?;
        replace_atomically_preserving_permissions(
            &configuration_target,
            &backup,
            &manifest.change_id,
        )?;
    }
    Ok(())
}

pub(crate) struct IntegratedTargetStates {
    pub(crate) rules: ArtifactState,
    pub(crate) configuration: ArtifactState,
}

impl IntegratedTargetStates {
    pub(crate) fn new(
        manifest: &ChangeManifest,
        configuration: &ConfigurationManifest,
        rules_current: Option<&[u8]>,
        configuration_current: Option<&[u8]>,
    ) -> Result<Self, AdapterTransactionError> {
        Ok(Self {
            rules: rules_state(manifest, rules_current)?,
            configuration: configuration_state(configuration, configuration_current)?,
        })
    }

    pub(crate) fn require_known(&self) -> Result<(), AdapterTransactionError> {
        self.rules.require_known()?;
        self.configuration.require_known()
    }
}

pub(crate) struct ArtifactState {
    pub(crate) backup: bool,
    pub(crate) candidate: bool,
}

impl ArtifactState {
    pub(crate) fn require_known(&self) -> Result<(), AdapterTransactionError> {
        if !self.backup && !self.candidate {
            return Err(AdapterTransactionError::ManagedFileChanged);
        }
        Ok(())
    }
}

pub(crate) fn rules_state(
    manifest: &ChangeManifest,
    current: Option<&[u8]>,
) -> Result<ArtifactState, AdapterTransactionError> {
    let candidate_hash = decode_hash(&manifest.candidate_sha256)?;
    Ok(ArtifactState {
        backup: matches_backup(manifest, current)?,
        candidate: current.is_some_and(|bytes| sha256(bytes) == candidate_hash),
    })
}

pub(crate) fn configuration_state(
    configuration: &ConfigurationManifest,
    current: Option<&[u8]>,
) -> Result<ArtifactState, AdapterTransactionError> {
    let backup_hash = decode_hash(&configuration.backup_sha256)?;
    let candidate_hash = decode_hash(&configuration.candidate_sha256)?;
    Ok(ArtifactState {
        backup: current.is_some_and(|bytes| sha256(bytes) == backup_hash),
        candidate: current.is_some_and(|bytes| sha256(bytes) == candidate_hash),
    })
}
