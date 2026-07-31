use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use nonproxy_adapter_api::{NormalizedPolicy, RenderedRules};

use crate::{
    AdapterTransactionError,
    atomic_file::{
        read_optional_bounded, remove_managed_file, remove_private_file, replace_atomically,
        sha256, write_private_new,
    },
    digest::{decode_hash, encode_hash, stable_identifier},
    identifier::validate_identifier,
    manifest::{ChangeManifest, remove_manifest},
    path_guard::{prepare_private_state_directory, validate_installation_path},
    recovery::recover_state,
    renderer_catalog,
    transaction_checks::{matches_backup, validate_installation},
    types::{
        AdapterInstallation, ApplyOutcome, ChangeInstallation, PreparedChange, RollbackOutcome,
        VerificationOutcome,
    },
};

pub(crate) const CHANGE_LIFETIME_MS: u64 = 10 * 60 * 1_000;
pub(crate) const MAXIMUM_ACTIVE_CHANGES: usize = 128;

pub struct AdapterTransactionManager {
    pub(crate) state_directory: PathBuf,
    pub(crate) mutation_gate: Mutex<()>,
}

impl AdapterTransactionManager {
    pub fn open(state_directory: impl Into<PathBuf>) -> Result<Self, AdapterTransactionError> {
        let state_directory = state_directory.into();
        prepare_private_state_directory(&state_directory)?;
        let state_directory = state_directory
            .canonicalize()
            .map_err(|_| AdapterTransactionError::StateDirectoryInvalid)?;
        let manager = Self {
            state_directory,
            mutation_gate: Mutex::new(()),
        };
        recover_state(&manager)?;
        Ok(manager)
    }

    pub fn prepare(
        &self,
        installation: &AdapterInstallation,
        operation_id: &str,
        normalized_policy: &[u8],
        now_unix_ms: u64,
    ) -> Result<PreparedChange, AdapterTransactionError> {
        let _guard = self
            .mutation_gate
            .lock()
            .map_err(|_| AdapterTransactionError::ChangeConflict)?;
        validate_installation(installation)?;
        validate_identifier(operation_id)?;
        let managed_rules_path = validate_installation_path(&installation.managed_rules_path)?;
        self.remove_expired_locked(now_unix_ms)?;

        let rendered = Self::render_candidate(installation, normalized_policy)?;
        let candidate_hash = *rendered.sha256();
        let change_id = stable_identifier(
            "change",
            &[operation_id.as_bytes(), installation.adapter_id.as_bytes()],
        );
        let candidate_path = self.candidate_path(&change_id);
        let manifest_path = self.manifest_path(&change_id);
        match ChangeManifest::read(&manifest_path) {
            Ok(existing) => {
                return self.replay_prepared(
                    existing,
                    installation,
                    operation_id,
                    &managed_rules_path,
                    &candidate_hash,
                );
            }
            Err(AdapterTransactionError::ChangeNotFound) => {}
            Err(error) => return Err(error),
        }
        let backup = read_optional_bounded(&managed_rules_path)?;
        let backup_hash = backup.as_deref().map(sha256);
        let backup_id = stable_identifier(
            "backup",
            &[
                change_id.as_bytes(),
                backup_hash.as_ref().map_or(&[][..], <[u8; 32]>::as_slice),
            ],
        );
        let expires_at_unix_ms = now_unix_ms
            .checked_add(CHANGE_LIFETIME_MS)
            .ok_or(AdapterTransactionError::ChangeConflict)?;
        let backup_path = self.backup_path(&backup_id);
        if self.manifest_count()? >= MAXIMUM_ACTIVE_CHANGES {
            return Err(AdapterTransactionError::ChangeConflict);
        }
        write_private_new(&candidate_path, rendered.bytes())?;
        if let Some(bytes) = backup.as_deref()
            && let Err(error) = write_private_new(&backup_path, bytes)
        {
            let _cleanup = fs::remove_file(&candidate_path);
            return Err(error);
        }
        let manifest = ChangeManifest {
            format_version: ChangeManifest::format_version(),
            change_id: change_id.clone(),
            backup_id: backup_id.clone(),
            operation_id: operation_id.to_owned(),
            adapter_id: installation.adapter_id.clone(),
            client: installation.client,
            client_version: Some(installation.client_version),
            managed_rules_path: managed_rules_path.to_string_lossy().into_owned(),
            candidate_sha256: encode_hash(&candidate_hash),
            backup_sha256: backup_hash.as_ref().map(encode_hash),
            backup_existed: backup.is_some(),
            prepared_at_unix_ms: now_unix_ms,
            expires_at_unix_ms,
            rule_count: rendered.rule_count(),
            configuration: None,
        };
        if let Err(error) = manifest.write_new(&manifest_path) {
            let _candidate_cleanup = fs::remove_file(&candidate_path);
            if backup.is_some() {
                let _backup_cleanup = fs::remove_file(&backup_path);
            }
            return Err(error);
        }
        Ok(PreparedChange {
            change_id,
            backup_id,
            candidate_sha256: candidate_hash,
            expires_at_unix_ms,
            rule_count: rendered.rule_count(),
            configuration_candidate_sha256: None,
            managed_rules_reference: None,
            direct_target: None,
        })
    }

    pub fn render_candidate(
        installation: &AdapterInstallation,
        normalized_policy: &[u8],
    ) -> Result<RenderedRules, AdapterTransactionError> {
        validate_installation(installation)?;
        let policy = NormalizedPolicy::from_json(normalized_policy)?;
        renderer_catalog::render(installation.client, installation.client_version, &policy)
            .map_err(AdapterTransactionError::from)
    }

    pub fn apply(
        &self,
        change_id: &str,
        expected_candidate_sha256: &[u8],
        now_unix_ms: u64,
    ) -> Result<ApplyOutcome, AdapterTransactionError> {
        let _guard = self
            .mutation_gate
            .lock()
            .map_err(|_| AdapterTransactionError::ChangeConflict)?;
        validate_identifier(change_id)?;
        let manifest = self.load_manifest(change_id)?;
        if manifest.configuration.is_some() {
            return Err(AdapterTransactionError::ChangeConflict);
        }
        if now_unix_ms > manifest.expires_at_unix_ms {
            return Err(AdapterTransactionError::ChangeExpired);
        }
        let candidate_hash = decode_hash(&manifest.candidate_sha256)?;
        if expected_candidate_sha256 != candidate_hash {
            return Err(AdapterTransactionError::CandidateHashMismatch);
        }
        let candidate = self.read_candidate(change_id, &candidate_hash)?;
        let target_path = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
        let current = read_optional_bounded(&target_path)?;
        if current
            .as_deref()
            .is_some_and(|bytes| sha256(bytes) == candidate_hash)
        {
            return Ok(ApplyOutcome {
                applied: true,
                replayed: true,
                candidate_sha256: candidate_hash,
                configuration_candidate_sha256: None,
            });
        }
        if !matches_backup(&manifest, current.as_deref())? {
            return Err(AdapterTransactionError::ManagedFileChanged);
        }
        replace_atomically(&target_path, &candidate, change_id)?;
        let written =
            read_optional_bounded(&target_path)?.ok_or(AdapterTransactionError::FileTransaction)?;
        if sha256(&written) != candidate_hash {
            return Err(AdapterTransactionError::FileTransaction);
        }
        Ok(ApplyOutcome {
            applied: true,
            replayed: false,
            candidate_sha256: candidate_hash,
            configuration_candidate_sha256: None,
        })
    }

    pub fn verify(&self, change_id: &str) -> Result<VerificationOutcome, AdapterTransactionError> {
        validate_identifier(change_id)?;
        let manifest = self.load_manifest(change_id)?;
        if manifest.configuration.is_some() {
            return crate::integrated_state::verify_integrated_manifest(&manifest);
        }
        let candidate_hash = decode_hash(&manifest.candidate_sha256)?;
        let target_path = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
        let current = read_optional_bounded(&target_path)?;
        Ok(VerificationOutcome {
            configuration_verified: current
                .as_deref()
                .is_some_and(|bytes| sha256(bytes) == candidate_hash),
            path_verified: false,
            candidate_sha256: candidate_hash,
            configuration_candidate_sha256: None,
        })
    }

    pub fn change_installation(
        &self,
        change_id: &str,
    ) -> Result<ChangeInstallation, AdapterTransactionError> {
        validate_identifier(change_id)?;
        let manifest = self.load_manifest(change_id)?;
        let client_version = manifest
            .client_version
            .ok_or(AdapterTransactionError::StateCorrupt)?;
        Ok(ChangeInstallation {
            adapter_id: manifest.adapter_id,
            client: manifest.client,
            client_version,
            managed_rules_path: PathBuf::from(manifest.managed_rules_path),
            main_configuration_path: manifest
                .configuration
                .as_ref()
                .map(|configuration| PathBuf::from(&configuration.path)),
            direct_target: manifest
                .configuration
                .as_ref()
                .map(|configuration| configuration.direct_target.clone()),
            requested_direct_target: manifest
                .configuration
                .and_then(|configuration| configuration.requested_direct_target),
        })
    }

    pub fn rollback(
        &self,
        change_id: &str,
        backup_id: &str,
    ) -> Result<RollbackOutcome, AdapterTransactionError> {
        let _guard = self
            .mutation_gate
            .lock()
            .map_err(|_| AdapterTransactionError::ChangeConflict)?;
        validate_identifier(change_id)?;
        validate_identifier(backup_id)?;
        let manifest = self.load_manifest(change_id)?;
        if manifest.backup_id != backup_id {
            return Err(AdapterTransactionError::ChangeConflict);
        }
        if manifest.configuration.is_some() {
            return crate::integrated_state::rollback_integrated_locked(self, &manifest);
        }
        let candidate_hash = decode_hash(&manifest.candidate_sha256)?;
        let target_path = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
        let current = read_optional_bounded(&target_path)?;
        if matches_backup(&manifest, current.as_deref())? {
            return Ok(RollbackOutcome {
                restored: true,
                replayed: true,
            });
        }
        if !current
            .as_deref()
            .is_some_and(|bytes| sha256(bytes) == candidate_hash)
        {
            return Err(AdapterTransactionError::ManagedFileChanged);
        }
        if manifest.backup_existed {
            let backup_hash = manifest
                .backup_sha256
                .as_deref()
                .ok_or(AdapterTransactionError::StateCorrupt)
                .and_then(decode_hash)?;
            let backup = read_optional_bounded(&self.backup_path(backup_id))?
                .ok_or(AdapterTransactionError::StateCorrupt)?;
            if sha256(&backup) != backup_hash {
                return Err(AdapterTransactionError::StateCorrupt);
            }
            replace_atomically(&target_path, &backup, change_id)?;
        } else {
            remove_managed_file(&target_path)?;
        }
        if !matches_backup(&manifest, read_optional_bounded(&target_path)?.as_deref())? {
            return Err(AdapterTransactionError::FileTransaction);
        }
        Ok(RollbackOutcome {
            restored: true,
            replayed: false,
        })
    }

    pub fn remove_change(&self, change_id: &str) -> Result<(), AdapterTransactionError> {
        let _guard = self
            .mutation_gate
            .lock()
            .map_err(|_| AdapterTransactionError::ChangeConflict)?;
        validate_identifier(change_id)?;
        let manifest = self.load_manifest(change_id)?;
        if manifest.configuration.is_some() {
            if !crate::integrated_state::integrated_targets_are_backups(&manifest)? {
                return Err(AdapterTransactionError::ChangeConflict);
            }
        } else {
            let target_path = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
            let current = read_optional_bounded(&target_path)?;
            if !matches_backup(&manifest, current.as_deref())? {
                return Err(AdapterTransactionError::ChangeConflict);
            }
        }
        remove_manifest(&self.manifest_path(change_id))?;
        remove_private_file(&self.candidate_path(change_id))?;
        if manifest.configuration.is_some() {
            remove_private_file(&self.configuration_candidate_path(change_id))?;
            remove_private_file(&self.configuration_backup_path(&manifest.backup_id))?;
        }
        if manifest.backup_existed {
            remove_private_file(&self.backup_path(&manifest.backup_id))?;
        }
        Ok(())
    }

    pub(crate) fn remove_expired_locked(
        &self,
        now_unix_ms: u64,
    ) -> Result<(), AdapterTransactionError> {
        for entry in fs::read_dir(self.state_directory.join("changes"))
            .map_err(|_| AdapterTransactionError::FileTransaction)?
        {
            let entry = entry.map_err(|_| AdapterTransactionError::FileTransaction)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let manifest = ChangeManifest::read(&path)?;
            if now_unix_ms <= manifest.expires_at_unix_ms {
                continue;
            }
            if manifest.configuration.is_some() {
                if !crate::integrated_state::integrated_targets_are_backups(&manifest)? {
                    continue;
                }
            } else {
                let current = read_optional_bounded(Path::new(&manifest.managed_rules_path))?;
                if !matches_backup(&manifest, current.as_deref())? {
                    continue;
                }
            }
            remove_manifest(&path)?;
            remove_private_file(&self.candidate_path(&manifest.change_id))?;
            if manifest.configuration.is_some() {
                remove_private_file(&self.configuration_candidate_path(&manifest.change_id))?;
                remove_private_file(&self.configuration_backup_path(&manifest.backup_id))?;
            }
            if manifest.backup_existed {
                remove_private_file(&self.backup_path(&manifest.backup_id))?;
            }
        }
        Ok(())
    }

    pub(crate) fn read_candidate(
        &self,
        change_id: &str,
        expected_hash: &[u8; 32],
    ) -> Result<Vec<u8>, AdapterTransactionError> {
        let candidate = read_optional_bounded(&self.candidate_path(change_id))?
            .ok_or(AdapterTransactionError::StateCorrupt)?;
        if sha256(&candidate) != *expected_hash {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        Ok(candidate)
    }

    fn replay_prepared(
        &self,
        manifest: ChangeManifest,
        installation: &AdapterInstallation,
        operation_id: &str,
        managed_rules_path: &Path,
        candidate_hash: &[u8; 32],
    ) -> Result<PreparedChange, AdapterTransactionError> {
        let identical = manifest.operation_id == operation_id
            && manifest.adapter_id == installation.adapter_id
            && manifest.client == installation.client
            && manifest.client_version == Some(installation.client_version)
            && Path::new(&manifest.managed_rules_path) == managed_rules_path
            && decode_hash(&manifest.candidate_sha256)? == *candidate_hash;
        if !identical {
            return Err(AdapterTransactionError::ChangeConflict);
        }
        let _candidate = self.read_candidate(&manifest.change_id, candidate_hash)?;
        match (manifest.backup_existed, manifest.backup_sha256.as_deref()) {
            (true, Some(expected)) => {
                let backup = read_optional_bounded(&self.backup_path(&manifest.backup_id))?
                    .ok_or(AdapterTransactionError::StateCorrupt)?;
                if sha256(&backup) != decode_hash(expected)? {
                    return Err(AdapterTransactionError::StateCorrupt);
                }
            }
            (false, None) => {}
            _ => return Err(AdapterTransactionError::StateCorrupt),
        }
        Ok(PreparedChange {
            change_id: manifest.change_id,
            backup_id: manifest.backup_id,
            candidate_sha256: *candidate_hash,
            expires_at_unix_ms: manifest.expires_at_unix_ms,
            rule_count: manifest.rule_count,
            configuration_candidate_sha256: None,
            managed_rules_reference: None,
            direct_target: None,
        })
    }

    pub(crate) fn load_manifest(
        &self,
        change_id: &str,
    ) -> Result<ChangeManifest, AdapterTransactionError> {
        let manifest = ChangeManifest::read(&self.manifest_path(change_id))?;
        if manifest.change_id != change_id {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        Ok(manifest)
    }

    pub(crate) fn manifest_count(&self) -> Result<usize, AdapterTransactionError> {
        let mut count = 0_usize;
        for entry in fs::read_dir(self.state_directory.join("changes"))
            .map_err(|_| AdapterTransactionError::FileTransaction)?
        {
            let entry = entry.map_err(|_| AdapterTransactionError::FileTransaction)?;
            if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
                count = count
                    .checked_add(1)
                    .ok_or(AdapterTransactionError::ChangeConflict)?;
            }
        }
        Ok(count)
    }

    pub(crate) fn candidate_path(&self, change_id: &str) -> PathBuf {
        self.state_directory
            .join("candidates")
            .join(format!("{change_id}.rules"))
    }

    pub(crate) fn configuration_candidate_path(&self, change_id: &str) -> PathBuf {
        self.state_directory
            .join("candidates")
            .join(format!("{change_id}.config"))
    }

    pub(crate) fn backup_path(&self, backup_id: &str) -> PathBuf {
        self.state_directory
            .join("backups")
            .join(format!("{backup_id}.rules"))
    }

    pub(crate) fn configuration_backup_path(&self, backup_id: &str) -> PathBuf {
        self.state_directory
            .join("backups")
            .join(format!("{backup_id}.config"))
    }

    pub(crate) fn manifest_path(&self, change_id: &str) -> PathBuf {
        self.state_directory
            .join("changes")
            .join(format!("{change_id}.json"))
    }
}
