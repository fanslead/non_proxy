use std::path::{Path, PathBuf};

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

pub struct IntegratedPreparation<'a> {
    pub(crate) installation: &'a AdapterInstallation,
    pub(crate) main_configuration_path: &'a Path,
    pub(crate) direct_target: Option<String>,
    pub(crate) operation_id: &'a str,
    pub(crate) normalized_policy: &'a [u8],
    pub(crate) expected_rules_sha256: &'a [u8],
    pub(crate) expected_configuration_sha256: &'a [u8],
    pub(crate) now_unix_ms: u64,
}

impl<'a> IntegratedPreparation<'a> {
    #[must_use]
    pub fn new(
        installation: &'a AdapterInstallation,
        main_configuration_path: &'a Path,
        operation_id: &'a str,
        normalized_policy: &'a [u8],
        expected_rules_sha256: &'a [u8],
        expected_configuration_sha256: &'a [u8],
        now_unix_ms: u64,
    ) -> Self {
        Self {
            installation,
            main_configuration_path,
            direct_target: None,
            operation_id,
            normalized_policy,
            expected_rules_sha256,
            expected_configuration_sha256,
            now_unix_ms,
        }
    }

    #[must_use]
    pub fn with_direct_target(mut self, direct_target: Option<String>) -> Self {
        self.direct_target = direct_target;
        self
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
    pub backup_id: String,
    pub adapter_id: String,
    pub client: AdapterClient,
    pub client_version: AdapterVersion,
    pub managed_rules_path: PathBuf,
    pub main_configuration_path: Option<PathBuf>,
    pub configuration_backup_sha256: Option<[u8; 32]>,
    pub configuration_candidate_sha256: Option<[u8; 32]>,
    pub direct_target: Option<String>,
    pub requested_direct_target: Option<String>,
}
