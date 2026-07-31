mod file;
mod http;
mod mihomo;
mod sing_box;
mod surge;

use nonproxy_adapter_api::AdapterClient;
use nonproxy_adapter_transaction::ChangeInstallation;

use crate::{AdapterHostError, detection::DetectedClient};

pub(crate) enum ReloadPlan {
    Surge(surge::SurgeReloadPlan),
    Mihomo(mihomo::MihomoReloadPlan),
    SingBox(sing_box::SingBoxReloadPlan),
}

impl ReloadPlan {
    pub(crate) fn prepare(
        detected: &DetectedClient,
        change: &ChangeInstallation,
        expected_configuration_sha256: Option<&[u8]>,
    ) -> Result<Self, AdapterHostError> {
        let main_configuration_path = change
            .main_configuration_path
            .as_deref()
            .ok_or(AdapterHostError::InstallationIncomplete)?;
        match detected.client {
            AdapterClient::Surge => Ok(Self::Surge(surge::SurgeReloadPlan::new(
                detected,
                change,
                main_configuration_path,
                expected_configuration_sha256,
            )?)),
            AdapterClient::Mihomo => Ok(Self::Mihomo(mihomo::MihomoReloadPlan::new(
                change,
                main_configuration_path,
                expected_configuration_sha256,
            )?)),
            AdapterClient::SingBox => Ok(Self::SingBox(sing_box::SingBoxReloadPlan::new(
                detected,
                change,
                main_configuration_path,
                expected_configuration_sha256,
            )?)),
        }
    }

    pub(crate) async fn preflight(&self) -> Result<(), AdapterHostError> {
        match self {
            Self::Surge(plan) => plan.preflight().await,
            Self::Mihomo(plan) => plan.preflight().await,
            Self::SingBox(plan) => plan.preflight(),
        }
    }

    pub(crate) async fn reload_applied(&self) -> Result<(), AdapterHostError> {
        match self {
            Self::Surge(plan) => plan.reload(true).await,
            Self::Mihomo(plan) => plan.reload(true).await,
            Self::SingBox(plan) => plan.reload(true).await,
        }
    }

    pub(crate) async fn reload_restored(&self) -> Result<(), AdapterHostError> {
        match self {
            Self::Surge(plan) => plan.reload(false).await,
            Self::Mihomo(plan) => plan.reload(false).await,
            Self::SingBox(plan) => plan.reload(false).await,
        }
    }
}
