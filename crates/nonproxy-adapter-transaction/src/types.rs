use std::path::PathBuf;

use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterInstallation {
    pub adapter_id: String,
    pub client: AdapterClient,
    pub client_version: AdapterVersion,
    pub managed_rules_path: PathBuf,
}

impl AdapterInstallation {
    pub fn new(
        adapter_id: impl Into<String>,
        client: AdapterClient,
        client_version: AdapterVersion,
        managed_rules_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            client,
            client_version,
            managed_rules_path: managed_rules_path.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedChange {
    pub change_id: String,
    pub backup_id: String,
    pub candidate_sha256: [u8; 32],
    pub expires_at_unix_ms: u64,
    pub rule_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyOutcome {
    pub applied: bool,
    pub replayed: bool,
    pub candidate_sha256: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationOutcome {
    pub configuration_verified: bool,
    pub path_verified: bool,
    pub candidate_sha256: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollbackOutcome {
    pub restored: bool,
    pub replayed: bool,
}
