use std::{fs, path::Path};

use nonproxy_adapter_integration::IntegrationPlan;

use crate::{
    AdapterTransactionError,
    atomic_file::{
        read_optional_bounded, remove_managed_file, replace_atomically,
        replace_atomically_preserving_permissions, sha256, write_private_new,
    },
    digest::{decode_hash, encode_hash, stable_identifier},
    host::AdapterTransactionManager,
    identifier::validate_identifier,
    integrated_state::{
        IntegratedTargetStates, configuration_state, rules_state, validate_manifest_backups,
        verify_integrated_manifest,
    },
    manifest::{ChangeManifest, ConfigurationManifest},
    path_guard::{validate_installation_path, validate_main_configuration_path},
    transaction_checks::validate_installation,
    types::{
        AdapterInstallation, ApplyOutcome, IntegratedCandidate, IntegratedPreparation,
        PreparedChange,
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

    pub fn prepare_integrated(
        &self,
        request: IntegratedPreparation<'_>,
    ) -> Result<PreparedChange, AdapterTransactionError> {
        let IntegratedPreparation {
            installation,
            main_configuration_path,
            direct_target,
            operation_id,
            normalized_policy,
            expected_rules_sha256,
            expected_configuration_sha256,
            now_unix_ms,
        } = request;
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
            direct_target.clone(),
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
                    IntegratedPreparationBinding {
                        managed_rules_path: &managed_rules_path,
                        main_configuration_path: &main_configuration_path,
                        requested_direct_target: direct_target.as_deref(),
                    },
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
                    requested_direct_target: direct_target,
                }),
            };
            manifest.write_new(&manifest_path)
        })();
        if let Err(error) = write_result {
            paths.remove_uncommitted(rules_backup.is_some());
            return Err(error);
        }
        Ok(PreparedChange {
            change_id,
            backup_id,
            candidate_sha256: rules_hash,
            expires_at_unix_ms,
            rule_count: preview.rendered_rules.rule_count(),
            configuration_candidate_sha256: Some(configuration_hash),
            managed_rules_reference: Some(preview.managed_rules_reference),
            direct_target: Some(preview.direct_target),
        })
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
        binding: IntegratedPreparationBinding<'_>,
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
            && Path::new(&manifest.managed_rules_path) == binding.managed_rules_path
            && Path::new(&configuration.path) == binding.main_configuration_path
            && decode_hash(&manifest.candidate_sha256)? == rules_hash
            && decode_hash(&configuration.candidate_sha256)? == preview.configuration_sha256
            && configuration.managed_rules_reference == preview.managed_rules_reference
            && configuration.direct_target == preview.direct_target
            && configuration.requested_direct_target.as_deref() == binding.requested_direct_target;
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
        Ok(PreparedChange {
            change_id: manifest.change_id,
            backup_id: manifest.backup_id,
            candidate_sha256: rules_hash,
            expires_at_unix_ms: manifest.expires_at_unix_ms,
            rule_count: manifest.rule_count,
            configuration_candidate_sha256: Some(preview.configuration_sha256),
            managed_rules_reference: Some(preview.managed_rules_reference.clone()),
            direct_target: Some(preview.direct_target.clone()),
        })
    }
}

struct IntegratedPreparationBinding<'a> {
    managed_rules_path: &'a Path,
    main_configuration_path: &'a Path,
    requested_direct_target: Option<&'a str>,
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

pub(crate) fn read_hashed_required(
    path: &Path,
    expected_hash: &[u8; 32],
) -> Result<Vec<u8>, AdapterTransactionError> {
    let bytes = read_optional_bounded(path)?.ok_or(AdapterTransactionError::StateCorrupt)?;
    if sha256(&bytes) != *expected_hash {
        return Err(AdapterTransactionError::StateCorrupt);
    }
    Ok(bytes)
}

pub(crate) fn restore_rules_backup(
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
