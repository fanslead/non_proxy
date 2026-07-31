use std::io;

use nonproxy_adapter_transaction::AdapterTransactionError;
use nonproxy_local_auth::LocalAuthError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterHostError {
    #[error("适配器宿主配置无效")]
    Configuration,
    #[error("适配器宿主文件操作失败")]
    File(#[source] io::Error),
    #[error("适配器宿主后台任务失败")]
    Task(#[source] tokio::task::JoinError),
    #[error("适配器宿主随机数生成失败")]
    Random(#[source] getrandom::Error),
    #[error("适配器安装项无效")]
    InstallationInvalid,
    #[error("适配器安装项不存在")]
    InstallationNotFound,
    #[error("适配器安装项缺少主配置，请重新登记")]
    InstallationIncomplete,
    #[error("适配器安装项冲突")]
    InstallationConflict,
    #[error("适配器安装项达到上限")]
    InstallationLimitReached,
    #[error("适配器客户端检测失败")]
    DetectionFailed,
    #[error("适配器客户端检测失败")]
    DetectionIo(#[source] io::Error),
    #[error("适配器客户端检测任务失败")]
    DetectionTask(#[source] tokio::task::JoinError),
    #[error("适配器候选未通过客户端原生校验")]
    CandidateValidationFailed,
    #[error("适配器客户端原生校验暂不可用")]
    CandidateValidationUnavailable,
    #[error("适配器候选原生校验文件操作失败")]
    CandidateValidationIo(#[source] io::Error),
    #[error("适配器候选原生校验任务失败")]
    CandidateValidationTask(#[source] tokio::task::JoinError),
    #[error("适配器客户端版本不受支持")]
    ClientUnsupported,
    #[error("适配器客户端版本在变更期间已经变化")]
    ClientVersionChanged,
    #[error("适配器安装路径或接入参数在变更期间已经变化")]
    InstallationChanged,
    #[error("适配器客户端没有可安全使用的公开控制入口")]
    ClientControlUnavailable,
    #[error("适配器客户端重载失败")]
    ClientReloadFailed,
    #[error("适配器客户端未确认受管配置已经载入")]
    ClientReloadUnconfirmed,
    #[error("适配器重载失败后的自动恢复未完整完成")]
    ClientRecoveryFailed,
    #[error("适配器目录状态已损坏")]
    CatalogCorrupt,
    #[error("适配器策略哈希不匹配")]
    PolicyHashMismatch,
    #[error("适配器宿主认证失败")]
    Authentication(#[from] LocalAuthError),
    #[error("适配器文件事务失败")]
    Transaction(#[from] AdapterTransactionError),
    #[error("适配器 RPC 服务失败")]
    Transport(#[from] tonic::transport::Error),
}

impl AdapterHostError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Configuration => "NP_ADAPTER_HOST_CONFIGURATION_INVALID",
            Self::File(_) => "NP_ADAPTER_HOST_FILE_FAILED",
            Self::Task(_) => "NP_ADAPTER_HOST_TASK_FAILED",
            Self::Random(_) => "NP_ADAPTER_HOST_RANDOM_FAILED",
            Self::InstallationInvalid => "NP_ADAPTER_INSTALLATION_INVALID",
            Self::InstallationNotFound => "NP_ADAPTER_INSTALLATION_NOT_FOUND",
            Self::InstallationIncomplete => "NP_ADAPTER_INSTALLATION_INCOMPLETE",
            Self::InstallationConflict => "NP_ADAPTER_INSTALLATION_CONFLICT",
            Self::InstallationLimitReached => "NP_ADAPTER_INSTALLATION_LIMIT_REACHED",
            Self::DetectionFailed | Self::DetectionIo(_) | Self::DetectionTask(_) => {
                "NP_ADAPTER_DETECTION_FAILED"
            }
            Self::CandidateValidationFailed => "NP_ADAPTER_CANDIDATE_VALIDATION_FAILED",
            Self::CandidateValidationUnavailable
            | Self::CandidateValidationIo(_)
            | Self::CandidateValidationTask(_) => "NP_ADAPTER_CANDIDATE_VALIDATION_UNAVAILABLE",
            Self::ClientUnsupported => "NP_ADAPTER_CLIENT_UNSUPPORTED",
            Self::ClientVersionChanged => "NP_ADAPTER_CLIENT_VERSION_CHANGED",
            Self::InstallationChanged => "NP_ADAPTER_INSTALLATION_CHANGED",
            Self::ClientControlUnavailable => "NP_ADAPTER_CLIENT_CONTROL_UNAVAILABLE",
            Self::ClientReloadFailed => "NP_ADAPTER_CLIENT_RELOAD_FAILED",
            Self::ClientReloadUnconfirmed => "NP_ADAPTER_CLIENT_RELOAD_UNCONFIRMED",
            Self::ClientRecoveryFailed => "NP_ADAPTER_CLIENT_RECOVERY_FAILED",
            Self::CatalogCorrupt => "NP_ADAPTER_CATALOG_CORRUPT",
            Self::PolicyHashMismatch => "NP_ADAPTER_POLICY_HASH_MISMATCH",
            Self::Authentication(error) => error.code(),
            Self::Transaction(error) => error.code(),
            Self::Transport(_) => "NP_ADAPTER_TRANSPORT_FAILED",
        }
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(Self::retryable_kind(self), RetryableKind::Yes)
    }

    const fn retryable_kind(&self) -> RetryableKind {
        match self {
            Self::File(_)
            | Self::Task(_)
            | Self::Random(_)
            | Self::DetectionFailed
            | Self::DetectionIo(_)
            | Self::DetectionTask(_)
            | Self::CandidateValidationUnavailable
            | Self::CandidateValidationIo(_)
            | Self::CandidateValidationTask(_)
            | Self::ClientControlUnavailable
            | Self::ClientReloadFailed
            | Self::ClientReloadUnconfirmed
            | Self::ClientRecoveryFailed
            | Self::Transport(_) => RetryableKind::Yes,
            Self::Transaction(AdapterTransactionError::FileTransaction) => RetryableKind::Yes,
            _ => RetryableKind::No,
        }
    }
}

#[derive(Clone, Copy)]
enum RetryableKind {
    No,
    Yes,
}
