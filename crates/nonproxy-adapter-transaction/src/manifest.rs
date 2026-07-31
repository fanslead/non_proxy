use std::path::Path;

use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
use serde::{Deserialize, Serialize};

use crate::{
    AdapterTransactionError,
    atomic_file::{read_optional_bounded, remove_private_file, write_private_new},
};

const MANIFEST_FORMAT_VERSION: u32 = 2;
const LEGACY_MANIFEST_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeManifest {
    pub format_version: u32,
    pub change_id: String,
    pub backup_id: String,
    pub operation_id: String,
    pub adapter_id: String,
    pub client: AdapterClient,
    #[serde(default)]
    pub client_version: Option<AdapterVersion>,
    pub managed_rules_path: String,
    pub candidate_sha256: String,
    pub backup_sha256: Option<String>,
    pub backup_existed: bool,
    pub prepared_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub rule_count: usize,
}

impl ChangeManifest {
    pub fn write_new(&self, path: &Path) -> Result<(), AdapterTransactionError> {
        let bytes = serde_json::to_vec(self).map_err(|_| AdapterTransactionError::StateCorrupt)?;
        write_private_new(path, &bytes)
    }

    pub fn read(path: &Path) -> Result<Self, AdapterTransactionError> {
        let Some(bytes) = read_optional_bounded(path)? else {
            return Err(AdapterTransactionError::ChangeNotFound);
        };
        let value: Self =
            serde_json::from_slice(&bytes).map_err(|_| AdapterTransactionError::StateCorrupt)?;
        if !matches!(
            value.format_version,
            LEGACY_MANIFEST_FORMAT_VERSION | MANIFEST_FORMAT_VERSION
        ) {
            return Err(AdapterTransactionError::StateCorrupt);
        }
        Ok(value)
    }

    pub const fn format_version() -> u32 {
        MANIFEST_FORMAT_VERSION
    }
}

pub(crate) fn remove_manifest(path: &Path) -> Result<(), AdapterTransactionError> {
    remove_private_file(path)
}
