use std::path::PathBuf;

use nonproxy_adapter_api::{AdapterClient, AdapterVersion, RenderedRules};

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
    pub configuration_candidate_sha256: Option<[u8; 32]>,
    pub managed_rules_reference: Option<String>,
    pub direct_target: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegratedCandidate {
    pub(crate) rendered_rules: RenderedRules,
    pub(crate) configuration_bytes: Vec<u8>,
    pub(crate) original_configuration_sha256: [u8; 32],
    pub(crate) configuration_sha256: [u8; 32],
    pub(crate) managed_rules_reference: String,
    pub(crate) direct_target: String,
}

impl IntegratedCandidate {
    #[must_use]
    pub fn rendered_rules(&self) -> &RenderedRules {
        &self.rendered_rules
    }

    #[must_use]
    pub fn configuration_bytes(&self) -> &[u8] {
        &self.configuration_bytes
    }

    #[must_use]
    pub const fn original_configuration_sha256(&self) -> &[u8; 32] {
        &self.original_configuration_sha256
    }

    #[must_use]
    pub const fn configuration_sha256(&self) -> &[u8; 32] {
        &self.configuration_sha256
    }

    #[must_use]
    pub fn managed_rules_reference(&self) -> &str {
        &self.managed_rules_reference
    }

    #[must_use]
    pub fn direct_target(&self) -> &str {
        &self.direct_target
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplyOutcome {
    pub applied: bool,
    pub replayed: bool,
    pub candidate_sha256: [u8; 32],
    pub configuration_candidate_sha256: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationOutcome {
    pub configuration_verified: bool,
    pub path_verified: bool,
    pub candidate_sha256: [u8; 32],
    pub configuration_candidate_sha256: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollbackOutcome {
    pub restored: bool,
    pub replayed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeInstallation {
    pub adapter_id: String,
    pub client: AdapterClient,
    pub client_version: AdapterVersion,
}
