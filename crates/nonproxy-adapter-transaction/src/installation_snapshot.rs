use std::path::PathBuf;

use crate::{
    AdapterTransactionError, digest::decode_hash, host::AdapterTransactionManager,
    identifier::validate_identifier, types::ChangeInstallation,
};

impl AdapterTransactionManager {
    pub fn change_installation(
        &self,
        change_id: &str,
    ) -> Result<ChangeInstallation, AdapterTransactionError> {
        validate_identifier(change_id)?;
        let manifest = self.load_manifest(change_id)?;
        let client_version = manifest
            .client_version
            .ok_or(AdapterTransactionError::StateCorrupt)?;
        let configuration_backup_sha256 = manifest
            .configuration
            .as_ref()
            .map(|configuration| decode_hash(&configuration.backup_sha256))
            .transpose()?;
        let configuration_candidate_sha256 = manifest
            .configuration
            .as_ref()
            .map(|configuration| decode_hash(&configuration.candidate_sha256))
            .transpose()?;
        Ok(ChangeInstallation {
            backup_id: manifest.backup_id,
            adapter_id: manifest.adapter_id,
            client: manifest.client,
            client_version,
            managed_rules_path: PathBuf::from(manifest.managed_rules_path),
            main_configuration_path: manifest
                .configuration
                .as_ref()
                .map(|configuration| PathBuf::from(&configuration.path)),
            configuration_backup_sha256,
            configuration_candidate_sha256,
            direct_target: manifest
                .configuration
                .as_ref()
                .map(|configuration| configuration.direct_target.clone()),
            requested_direct_target: manifest
                .configuration
                .and_then(|configuration| configuration.requested_direct_target),
        })
    }
}
