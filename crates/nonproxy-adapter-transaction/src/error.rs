use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterTransactionError {
    #[error("适配器安装描述无效")]
    InstallationInvalid,
    #[error("适配器状态目录无效")]
    StateDirectoryInvalid,
    #[error("适配器托管规则路径无效")]
    ManagedPathInvalid,
    #[error("适配器托管规则已被外部修改")]
    ManagedFileChanged,
    #[error("适配器变更标识无效")]
    ChangeIdInvalid,
    #[error("适配器变更不存在")]
    ChangeNotFound,
    #[error("适配器变更已经过期")]
    ChangeExpired,
    #[error("适配器候选哈希不匹配")]
    CandidateHashMismatch,
    #[error("适配器候选规则无效")]
    CandidateInvalid,
    #[error("适配器主配置无效或不受支持")]
    MainConfigurationInvalid,
    #[error("适配器主配置与 NonProxy 接入节点冲突")]
    MainConfigurationConflict,
    #[error("适配器主配置包含多个可用直连出口")]
    DirectTargetAmbiguous,
    #[error("适配器直连出口无效")]
    DirectTargetInvalid,
    #[error("适配器变更状态冲突")]
    ChangeConflict,
    #[error("适配器文件事务失败")]
    FileTransaction,
    #[error("适配器状态数据已损坏")]
    StateCorrupt,
}

impl AdapterTransactionError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InstallationInvalid => "NP_ADAPTER_INSTALLATION_INVALID",
            Self::StateDirectoryInvalid => "NP_ADAPTER_STATE_DIRECTORY_INVALID",
            Self::ManagedPathInvalid => "NP_ADAPTER_MANAGED_PATH_INVALID",
            Self::ManagedFileChanged => "NP_ADAPTER_MANAGED_FILE_CHANGED",
            Self::ChangeIdInvalid => "NP_ADAPTER_CHANGE_ID_INVALID",
            Self::ChangeNotFound => "NP_ADAPTER_CHANGE_NOT_FOUND",
            Self::ChangeExpired => "NP_ADAPTER_CHANGE_EXPIRED",
            Self::CandidateHashMismatch => "NP_ADAPTER_CANDIDATE_HASH_MISMATCH",
            Self::CandidateInvalid => "NP_ADAPTER_CANDIDATE_INVALID",
            Self::MainConfigurationInvalid => "NP_ADAPTER_MAIN_CONFIGURATION_INVALID",
            Self::MainConfigurationConflict => "NP_ADAPTER_MAIN_CONFIGURATION_CONFLICT",
            Self::DirectTargetAmbiguous => "NP_ADAPTER_DIRECT_TARGET_AMBIGUOUS",
            Self::DirectTargetInvalid => "NP_ADAPTER_DIRECT_TARGET_INVALID",
            Self::ChangeConflict => "NP_ADAPTER_CHANGE_CONFLICT",
            Self::FileTransaction => "NP_ADAPTER_FILE_TRANSACTION_FAILED",
            Self::StateCorrupt => "NP_ADAPTER_STATE_CORRUPT",
        }
    }
}

impl From<nonproxy_adapter_api::AdapterContractError> for AdapterTransactionError {
    fn from(_: nonproxy_adapter_api::AdapterContractError) -> Self {
        Self::CandidateInvalid
    }
}

impl From<nonproxy_adapter_integration::AdapterIntegrationError> for AdapterTransactionError {
    fn from(error: nonproxy_adapter_integration::AdapterIntegrationError) -> Self {
        use nonproxy_adapter_integration::AdapterIntegrationError as IntegrationError;

        match error {
            IntegrationError::ConfigurationTooLarge | IntegrationError::ConfigurationInvalid => {
                Self::MainConfigurationInvalid
            }
            IntegrationError::IntegrationIdInvalid => Self::InstallationInvalid,
            IntegrationError::ManagedPathOutsideConfiguration
            | IntegrationError::ManagedPathInvalid => Self::ManagedPathInvalid,
            IntegrationError::IntegrationConflict => Self::MainConfigurationConflict,
            IntegrationError::DirectTargetAmbiguous => Self::DirectTargetAmbiguous,
            IntegrationError::DirectTargetInvalid => Self::DirectTargetInvalid,
        }
    }
}
