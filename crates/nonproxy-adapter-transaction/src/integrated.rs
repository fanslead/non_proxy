use std::{fs, path::Path};

use nonproxy_adapter_integration::IntegrationPlan;

use crate::{
    AdapterTransactionError,
    atomic_file::{
        read_optional_bounded, remove_managed_file, replace_atomically,
        replace_atomically_preserving_permissions, sha256, write_private_new,
    },
    digest::{decode_hash, encode_hash, stable_identifier},
    host::{AdapterTransactionManager, matches_backup, validate_installation},
    identifier::validate_identifier,
    manifest::{ChangeManifest, ConfigurationManifest},
    path_guard::{validate_installation_path, validate_main_configuration_path},
    types::{
        AdapterInstallation, ApplyOutcome, IntegratedCandidate, PreparedChange, RollbackOutcome,
        VerificationOutcome,
    },
};

impl AdapterTransactionManager {
    pub fn preview_integrated(
        installation: &AdapterInstallation,
        main_configuration_path: &Path,
        direct_target: Option<String>,
        normalized_policy: &[u8],
    ) -> Result<IntegratedCandidate, AdapterTransactionError> {
        validate_installation(installation)?;
        let managed_rules_path = validate_installation_path(&installation.managed_rules_path)?;
        let main_configuration_path = validate_main_configuration_path(main_configuration_path)?;
        if managed_rules_path == main_configuration_path {
            return Err(AdapterTransactionError::InstallationInvalid);
        }
        let configuration = read_optional_bounded(&main_configuration_path)?
            .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
        let original_configuration_sha256 = sha256(&configuration);
        let rendered_rules = Self::render_candidate(installation, normalized_policy)?;
        let integration = IntegrationPlan::new(
            installation.client,
            installation.adapter_id.clone(),
            main_configuration_path,
            managed_rules_path,
            direct_target,
        )
        .map_err(AdapterTransactionError::from)?
        .patch(&configuration)
        .map_err(AdapterTransactionError::from)?;
        Ok(IntegratedCandidate {
            rendered_rules,
            configuration_bytes: integration.bytes().to_vec(),
            original_configuration_sha256,
            configuration_sha256: *integration.sha256(),
            managed_rules_reference: integration.managed_rules_reference().to_owned(),
            direct_target: integration.direct_target().to_owned(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_integrated(
        &self,
        installation: &AdapterInstallation,
        main_configuration_path: &Path,
        direct_target: Option<String>,
        operation_id: &str,
        normalized_policy: &[u8],
        expected_rules_sha256: &[u8],
        expected_configuration_sha256: &[u8],
        now_unix_ms: u64,
    ) -> Result<PreparedChange, AdapterTransactionError> {
        let _guard = self
            .mutation_gate
            .lock()
            .map_err(|_| AdapterTransactionError::ChangeConflict)?;
        validate_identifier(operation_id)?;
        let managed_rules_path = validate_installation_path(&installation.managed_rules_path)?;
        let main_configuration_path = validate_main_configuration_path(main_configuration_path)?;
        self.remove_expired_locked(now_unix_ms)?;
        let preview = Self::preview_integrated(
            installation,
            &main_configuration_path,
            direct_target,
            normalized_policy,
        )?;
        let rules_hash = *preview.rendered_rules.sha256();
        let configuration_hash = preview.configuration_sha256;
        if expected_rules_sha256 != rules_hash
            || expected_configuration_sha256 != configuration_hash
        {
            return Err(AdapterTransactionError::CandidateHashMismatch);
        }
        let change_id = stable_identifier(
            "change",
            &[operation_id.as_bytes(), installation.adapter_id.as_bytes()],
        );
        let manifest_path = self.manifest_path(&change_id);
        match ChangeManifest::read(&manifest_path) {
            Ok(existing) => {
                return self.replay_integrated_prepared(
                    existing,
                    installation,
                    operation_id,
                    &managed_rules_path,
                    &main_configuration_path,
                    &preview,
                );
            }
            Err(AdapterTransactionError::ChangeNotFound) => {}
            Err(error) => return Err(error),
        }
        if self.manifest_count()? >= super::host::MAXIMUM_ACTIVE_CHANGES {
            return Err(AdapterTransactionError::ChangeConflict);
        }
        let rules_backup = read_optional_bounded(&managed_rules_path)?;
        let rules_backup_hash = rules_backup.as_deref().map(sha256);
        let configuration_backup = read_optional_bounded(&main_configuration_path)?
            .ok_or(AdapterTransactionError::ManagedPathInvalid)?;
        let configuration_backup_hash = sha256(&configuration_backup);
        if configuration_backup_hash != preview.original_configuration_sha256 {
            return Err(AdapterTransactionError::ManagedFileChanged);
        }
        let backup_id = stable_identifier(
            "backup",
            &[
                change_id.as_bytes(),
                rules_backup_hash
                    .as_ref()
                    .map_or(&[][..], <[u8; 32]>::as_slice),
                configuration_backup_hash.as_slice(),
            ],
        );
        let expires_at_unix_ms = now_unix_ms
            .checked_add(super::host::CHANGE_LIFETIME_MS)
            .ok_or(AdapterTransactionError::ChangeConflict)?;
        let paths = IntegratedStatePaths::new(self, &change_id, &backup_id);
        let write_result = (|| {
            write_private_new(&paths.rules_candidate, preview.rendered_rules.bytes())?;
            write_private_new(&paths.configuration_candidate, &preview.configuration_bytes)?;
            if let Some(bytes) = rules_backup.as_deref() {
                write_private_new(&paths.rules_backup, bytes)?;
            }
            write_private_new(&paths.configuration_backup, &configuration_backup)?;
            let manifest = ChangeManifest {
                format_version: ChangeManifest::format_version(),
                change_id: change_id.clone(),
                backup_id: backup_id.clone(),
                operation_id: operation_id.to_owned(),
                adapter_id: installation.adapter_id.clone(),
                client: installation.client,
                client_version: Some(installation.client_version),
                managed_rules_path: managed_rules_path.to_string_lossy().into_owned(),
                candidate_sha256: encode_hash(&rules_hash),
                backup_sha256: rules_backup_hash.as_ref().map(encode_hash),
                backup_existed: rules_backup.is_some(),
                prepared_at_unix_ms: now_unix_ms,
                expires_at_unix_ms,
                rule_count: preview.rendered_rules.rule_count(),
                configuration: Some(ConfigurationManifest {
                    path: main_configuration_path.to_string_lossy().into_owned(),
                    candidate_sha256: encode_hash(&configuration_hash),
                    backup_sha256: encode_hash(&configuration_backup_hash),
                    managed_rules_reference: preview.managed_rules_reference.clone(),
                    direct_target: preview.direct_target.clone(),
                }),
            };
            manifest.write_new(&manifest_path)
        })();
        if let Err(error) = write_result {
            paths.remove_uncommitted(rules_backup.is_some());
            return Err(error);
        }
        Ok(prepared_from_manifest_values(
            change_id,
            backup_id,
            rules_hash,
            configuration_hash,
            expires_at_unix_ms,
            preview.rendered_rules.rule_count(),
            preview.managed_rules_reference,
            preview.direct_target,
        ))
    }

    pub fn apply_integrated(
        &self,
        change_id: &str,
        expected_rules_sha256: &[u8],
        expected_configuration_sha256: &[u8],
        now_unix_ms: u64,
    ) -> Result<ApplyOutcome, AdapterTransactionError> {
        let _guard = self
            .mutation_gate
            .lock()
            .map_err(|_| AdapterTransactionError::ChangeConflict)?;
        validate_identifier(change_id)?;
        let manifest = self.load_manifest(change_id)?;
        if now_unix_ms > manifest.expires_at_unix_ms {
            return Err(AdapterTransactionError::ChangeExpired);
        }
        let configuration = manifest
            .configuration
            .as_ref()
            .ok_or(AdapterTransactionError::ChangeConflict)?;
        let rules_hash = decode_hash(&manifest.candidate_sha256)?;
        let configuration_hash = decode_hash(&configuration.candidate_sha256)?;
        if expected_rules_sha256 != rules_hash
            || expected_configuration_sha256 != configuration_hash
        {
            return Err(AdapterTransactionError::CandidateHashMismatch);
        }
        let paths = IntegratedStatePaths::new(self, change_id, &manifest.backup_id);
        let rules_candidate = self.read_candidate(change_id, &rules_hash)?;
        let configuration_candidate =
            read_hashed_required(&paths.configuration_candidate, &configuration_hash)?;
        let rules_target = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
        let configuration_target =
            validate_main_configuration_path(Path::new(&configuration.path))?;
        let rules_current = read_optional_bounded(&rules_target)?;
        let configuration_current = read_optional_bounded(&configuration_target)?;
        let states = IntegratedTargetStates::new(
            &manifest,
            configuration,
            rules_current.as_deref(),
            configuration_current.as_deref(),
        )?;
        states.require_known()?;
        if states.rules.candidate && states.configuration.candidate {
            return Ok(ApplyOutcome {
                applied: true,
                replayed: true,
                candidate_sha256: rules_hash,
                configuration_candidate_sha256: Some(configuration_hash),
            });
        }
        let mut wrote_rules = false;
        if !states.rules.candidate {
            let current = read_optional_bounded(&rules_target)?;
            let fresh = rules_state(&manifest, current.as_deref())?;
            fresh.require_known()?;
            if !fresh.candidate {
                replace_atomically(&rules_target, &rules_candidate, change_id)?;
                wrote_rules = true;
            }
        }
        if !states.configuration.candidate {
            let current = read_optional_bounded(&configuration_target)?;
            let fresh = configuration_state(configuration, current.as_deref())?;
            if let Err(error) = fresh.require_known() {
                if wrote_rules
                    && restore_rules_backup(self, &manifest, &rules_target, change_id).is_err()
                {
                    return Err(AdapterTransactionError::FileTransaction);
                }
                return Err(error);
            }
            if !fresh.candidate
                && let Err(error) = replace_atomically_preserving_permissions(
                    &configuration_target,
                    &configuration_candidate,
                    change_id,
                )
            {
                if wrote_rules
                    && restore_rules_backup(self, &manifest, &rules_target, change_id).is_err()
                {
                    return Err(AdapterTransactionError::FileTransaction);
                }
                return Err(error);
            }
        }
        let verified = verify_integrated_manifest(&manifest)?;
        if !verified.configuration_verified {
            return Err(AdapterTransactionError::FileTransaction);
        }
        Ok(ApplyOutcome {
            applied: true,
            replayed: false,
            candidate_sha256: rules_hash,
            configuration_candidate_sha256: Some(configuration_hash),
        })
    }

    fn replay_integrated_prepared(
        &self,
        manifest: ChangeManifest,
        installation: &AdapterInstallation,
        operation_id: &str,
        managed_rules_path: &Path,
        main_configuration_path: &Path,
        preview: &IntegratedCandidate,
    ) -> Result<PreparedChange, AdapterTransactionError> {
        let configuration = manifest
            .configuration
            .as_ref()
            .ok_or(AdapterTransactionError::ChangeConflict)?;
        let rules_hash = *preview.rendered_rules.sha256();
        let identical = manifest.operation_id == operation_id
            && manifest.adapter_id == installation.adapter_id
            && manifest.client == installation.client
            && manifest.client_version == Some(installation.client_version)
            && Path::new(&manifest.managed_rules_path) == managed_rules_path
            && Path::new(&configuration.path) == main_configuration_path
            && decode_hash(&manifest.candidate_sha256)? == rules_hash
            && decode_hash(&configuration.candidate_sha256)? == preview.configuration_sha256
            && configuration.managed_rules_reference == preview.managed_rules_reference
            && configuration.direct_target == preview.direct_target;
        if !identical {
            return Err(AdapterTransactionError::ChangeConflict);
        }
        let paths = IntegratedStatePaths::new(self, &manifest.change_id, &manifest.backup_id);
        let _rules_candidate = self.read_candidate(&manifest.change_id, &rules_hash)?;
        let _configuration_candidate = read_hashed_required(
            &paths.configuration_candidate,
            &preview.configuration_sha256,
        )?;
        validate_manifest_backups(self, &manifest)?;
        Ok(prepared_from_manifest_values(
            manifest.change_id,
            manifest.backup_id,
            rules_hash,
            preview.configuration_sha256,
            manifest.expires_at_unix_ms,
            manifest.rule_count,
            preview.managed_rules_reference.clone(),
            preview.direct_target.clone(),
        ))
    }
}

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

struct IntegratedTargetStates {
    rules: ArtifactState,
    configuration: ArtifactState,
}

impl IntegratedTargetStates {
    fn new(
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

    fn require_known(&self) -> Result<(), AdapterTransactionError> {
        self.rules.require_known()?;
        self.configuration.require_known()
    }
}

struct ArtifactState {
    backup: bool,
    candidate: bool,
}

impl ArtifactState {
    fn require_known(&self) -> Result<(), AdapterTransactionError> {
        if !self.backup && !self.candidate {
            return Err(AdapterTransactionError::ManagedFileChanged);
        }
        Ok(())
    }
}

fn rules_state(
    manifest: &ChangeManifest,
    current: Option<&[u8]>,
) -> Result<ArtifactState, AdapterTransactionError> {
    let candidate_hash = decode_hash(&manifest.candidate_sha256)?;
    Ok(ArtifactState {
        backup: matches_backup(manifest, current)?,
        candidate: current.is_some_and(|bytes| sha256(bytes) == candidate_hash),
    })
}

fn configuration_state(
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

struct IntegratedStatePaths {
    rules_candidate: std::path::PathBuf,
    configuration_candidate: std::path::PathBuf,
    rules_backup: std::path::PathBuf,
    configuration_backup: std::path::PathBuf,
}

impl IntegratedStatePaths {
    fn new(manager: &AdapterTransactionManager, change_id: &str, backup_id: &str) -> Self {
        Self {
            rules_candidate: manager.candidate_path(change_id),
            configuration_candidate: manager.configuration_candidate_path(change_id),
            rules_backup: manager.backup_path(backup_id),
            configuration_backup: manager.configuration_backup_path(backup_id),
        }
    }

    fn remove_uncommitted(&self, rules_backup_existed: bool) {
        let _rules_candidate = fs::remove_file(&self.rules_candidate);
        let _configuration_candidate = fs::remove_file(&self.configuration_candidate);
        if rules_backup_existed {
            let _rules_backup = fs::remove_file(&self.rules_backup);
        }
        let _configuration_backup = fs::remove_file(&self.configuration_backup);
    }
}

fn read_hashed_required(
    path: &Path,
    expected_hash: &[u8; 32],
) -> Result<Vec<u8>, AdapterTransactionError> {
    let bytes = read_optional_bounded(path)?.ok_or(AdapterTransactionError::StateCorrupt)?;
    if sha256(&bytes) != *expected_hash {
        return Err(AdapterTransactionError::StateCorrupt);
    }
    Ok(bytes)
}

fn restore_rules_backup(
    manager: &AdapterTransactionManager,
    manifest: &ChangeManifest,
    target: &Path,
    change_id: &str,
) -> Result<(), AdapterTransactionError> {
    if manifest.backup_existed {
        let backup_hash = manifest
            .backup_sha256
            .as_deref()
            .ok_or(AdapterTransactionError::StateCorrupt)
            .and_then(decode_hash)?;
        let backup = read_hashed_required(&manager.backup_path(&manifest.backup_id), &backup_hash)?;
        replace_atomically(target, &backup, change_id)
    } else {
        remove_managed_file(target)
    }
}

#[allow(clippy::too_many_arguments)]
fn prepared_from_manifest_values(
    change_id: String,
    backup_id: String,
    rules_hash: [u8; 32],
    configuration_hash: [u8; 32],
    expires_at_unix_ms: u64,
    rule_count: usize,
    managed_rules_reference: String,
    direct_target: String,
) -> PreparedChange {
    PreparedChange {
        change_id,
        backup_id,
        candidate_sha256: rules_hash,
        expires_at_unix_ms,
        rule_count,
        configuration_candidate_sha256: Some(configuration_hash),
        managed_rules_reference: Some(managed_rules_reference),
        direct_target: Some(direct_target),
    }
}
