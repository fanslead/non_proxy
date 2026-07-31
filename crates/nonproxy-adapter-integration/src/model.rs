use std::path::{Path, PathBuf};

use nonproxy_adapter_api::AdapterClient;
use sha2::{Digest, Sha256};

use crate::{AdapterIntegrationError, mihomo, path::managed_reference, sing_box, surge};

const MAXIMUM_CONFIGURATION_BYTES: usize = 2 * 1024 * 1024;
const MAXIMUM_IDENTIFIER_BYTES: usize = 96;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationPlan {
    client: AdapterClient,
    integration_id: String,
    configuration_path: PathBuf,
    managed_rules_path: PathBuf,
    direct_target: Option<String>,
}

impl IntegrationPlan {
    pub fn new(
        client: AdapterClient,
        integration_id: impl Into<String>,
        configuration_path: impl Into<PathBuf>,
        managed_rules_path: impl Into<PathBuf>,
        direct_target: Option<String>,
    ) -> Result<Self, AdapterIntegrationError> {
        let integration_id = integration_id.into();
        validate_identifier(&integration_id)?;
        let configuration_path = configuration_path.into();
        let managed_rules_path = managed_rules_path.into();
        let _reference = managed_reference(&configuration_path, &managed_rules_path)?;
        let direct_target = normalize_direct_target(direct_target)?;
        Ok(Self {
            client,
            integration_id,
            configuration_path,
            managed_rules_path,
            direct_target,
        })
    }

    pub fn patch(
        &self,
        configuration: &[u8],
    ) -> Result<PatchedConfiguration, AdapterIntegrationError> {
        validate_size(configuration)?;
        let reference = managed_reference(&self.configuration_path, &self.managed_rules_path)?;
        let (bytes, direct_target) = match self.client {
            AdapterClient::Surge => surge::patch(
                configuration,
                &self.integration_id,
                &reference,
                self.direct_target.as_deref(),
            )?,
            AdapterClient::Mihomo => mihomo::patch(
                configuration,
                &self.integration_id,
                &reference,
                self.direct_target.as_deref(),
            )?,
            AdapterClient::SingBox => sing_box::patch(
                configuration,
                &self.integration_id,
                &reference,
                self.direct_target.as_deref(),
            )?,
        };
        validate_size(&bytes)?;
        let changed = bytes != configuration;
        let sha256 = Sha256::digest(&bytes).into();
        Ok(PatchedConfiguration {
            bytes,
            changed,
            reference_name: reference_name(&self.integration_id),
            managed_rules_reference: reference,
            direct_target,
            sha256,
        })
    }

    pub fn inspect(
        &self,
        configuration: &[u8],
    ) -> Result<IntegrationInspection, AdapterIntegrationError> {
        validate_size(configuration)?;
        let reference = managed_reference(&self.configuration_path, &self.managed_rules_path)?;
        let (integrated, direct_target) = match self.client {
            AdapterClient::Surge => surge::inspect(
                configuration,
                &self.integration_id,
                &reference,
                self.direct_target.as_deref(),
            )?,
            AdapterClient::Mihomo => mihomo::inspect(
                configuration,
                &self.integration_id,
                &reference,
                self.direct_target.as_deref(),
            )?,
            AdapterClient::SingBox => sing_box::inspect(
                configuration,
                &self.integration_id,
                &reference,
                self.direct_target.as_deref(),
            )?,
        };
        Ok(IntegrationInspection {
            integrated,
            reference_name: reference_name(&self.integration_id),
            managed_rules_reference: reference,
            direct_target,
            configuration_sha256: Sha256::digest(configuration).into(),
        })
    }

    #[must_use]
    pub fn configuration_path(&self) -> &Path {
        &self.configuration_path
    }

    #[must_use]
    pub fn managed_rules_path(&self) -> &Path {
        &self.managed_rules_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchedConfiguration {
    bytes: Vec<u8>,
    changed: bool,
    reference_name: String,
    managed_rules_reference: String,
    direct_target: String,
    sha256: [u8; 32],
}

impl PatchedConfiguration {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn changed(&self) -> bool {
        self.changed
    }

    #[must_use]
    pub fn reference_name(&self) -> &str {
        &self.reference_name
    }

    #[must_use]
    pub fn managed_rules_reference(&self) -> &str {
        &self.managed_rules_reference
    }

    #[must_use]
    pub fn direct_target(&self) -> &str {
        &self.direct_target
    }

    #[must_use]
    pub const fn sha256(&self) -> &[u8; 32] {
        &self.sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationInspection {
    pub integrated: bool,
    pub reference_name: String,
    pub managed_rules_reference: String,
    pub direct_target: String,
    pub configuration_sha256: [u8; 32],
}

pub(crate) fn reference_name(integration_id: &str) -> String {
    format!("nonproxy-{integration_id}")
}

fn validate_identifier(value: &str) -> Result<(), AdapterIntegrationError> {
    if value.is_empty()
        || value.len() > MAXIMUM_IDENTIFIER_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AdapterIntegrationError::IntegrationIdInvalid);
    }
    Ok(())
}

fn normalize_direct_target(
    value: Option<String>,
) -> Result<Option<String>, AdapterIntegrationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| character.is_control())
    {
        return Err(AdapterIntegrationError::DirectTargetInvalid);
    }
    Ok(Some(value.to_owned()))
}

fn validate_size(bytes: &[u8]) -> Result<(), AdapterIntegrationError> {
    if bytes.len() > MAXIMUM_CONFIGURATION_BYTES {
        return Err(AdapterIntegrationError::ConfigurationTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nonproxy_adapter_api::AdapterClient;

    use crate::{AdapterIntegrationError, IntegrationPlan};

    #[test]
    fn public_plan_is_idempotent_and_reports_sanitized_metadata() {
        let plan = IntegrationPlan::new(
            AdapterClient::Surge,
            "surge-primary",
            "/client/main.conf",
            "/client/rules/nonproxy.list",
            None,
        )
        .unwrap_or_else(|error| panic!("接入计划创建失败: {error}"));
        let original = b"[Rule]\nFINAL,Proxy\n";
        let patched = plan
            .patch(original)
            .unwrap_or_else(|error| panic!("主配置候选失败: {error}"));
        assert!(patched.changed());
        assert_eq!(patched.reference_name(), "nonproxy-surge-primary");
        assert_eq!(patched.managed_rules_reference(), "./rules/nonproxy.list");
        assert_eq!(patched.direct_target(), "DIRECT");
        assert_eq!(patched.sha256().len(), 32);
        assert!(
            plan.inspect(patched.bytes())
                .is_ok_and(|value| value.integrated)
        );
        assert!(
            plan.patch(patched.bytes())
                .is_ok_and(|value| !value.changed())
        );
    }

    #[test]
    fn rejects_unbounded_configuration_before_parsing() {
        let plan = IntegrationPlan::new(
            AdapterClient::Surge,
            "surge-primary",
            "/client/main.conf",
            "/client/nonproxy.list",
            None,
        )
        .unwrap_or_else(|error| panic!("接入计划创建失败: {error}"));
        let oversized = vec![b'a'; 2 * 1024 * 1024 + 1];
        assert_eq!(
            plan.patch(&oversized),
            Err(AdapterIntegrationError::ConfigurationTooLarge)
        );
    }
}
