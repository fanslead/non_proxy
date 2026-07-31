use std::path::PathBuf;

use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredInstallation {
    pub adapter_id: String,
    pub client: AdapterClient,
    pub client_version: AdapterVersion,
    pub executable_path: PathBuf,
    pub managed_rules_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_configuration_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_target: Option<String>,
}
