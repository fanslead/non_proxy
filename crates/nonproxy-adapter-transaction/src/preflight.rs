use std::path::Path;

use crate::{
    AdapterTransactionError,
    atomic_file::{read_optional_bounded, sha256},
    digest::decode_hash,
    host::AdapterTransactionManager,
    identifier::validate_identifier,
    integrated::read_hashed_required,
    integrated_state::IntegratedTargetStates,
    path_guard::{validate_installation_path, validate_main_configuration_path},
    transaction_checks::matches_backup,
};

impl AdapterTransactionManager {
    pub fn preflight_apply(
        &self,
        change_id: &str,
        expected_candidate_sha256: &[u8],
        now_unix_ms: u64,
    ) -> Result<(), AdapterTransactionError> {
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
        let _candidate = self.read_candidate(change_id, &candidate_hash)?;
        let target_path = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
        let current = read_optional_bounded(&target_path)?;
        if current
            .as_deref()
            .is_some_and(|bytes| sha256(bytes) == candidate_hash)
            || matches_backup(&manifest, current.as_deref())?
        {
            return Ok(());
        }
        Err(AdapterTransactionError::ManagedFileChanged)
    }

    pub fn preflight_apply_integrated(
        &self,
        change_id: &str,
        expected_rules_sha256: &[u8],
        expected_configuration_sha256: &[u8],
        now_unix_ms: u64,
    ) -> Result<(), AdapterTransactionError> {
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
        let _rules_candidate = self.read_candidate(change_id, &rules_hash)?;
        let _configuration_candidate = read_hashed_required(
            &self.configuration_candidate_path(change_id),
            &configuration_hash,
        )?;
        let rules_target = validate_installation_path(Path::new(&manifest.managed_rules_path))?;
        let configuration_target =
            validate_main_configuration_path(Path::new(&configuration.path))?;
        let rules_current = read_optional_bounded(&rules_target)?;
        let configuration_current = read_optional_bounded(&configuration_target)?;
        IntegratedTargetStates::new(
            &manifest,
            configuration,
            rules_current.as_deref(),
            configuration_current.as_deref(),
        )?
        .require_known()
    }
}
