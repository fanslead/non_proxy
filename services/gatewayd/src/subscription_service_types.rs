use nonproxy_storage::StorageError;
use nonproxy_subscription::SubscriptionFetchError;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::{
    GatewayError, credential_store::CredentialWriteFailure,
    subscription_prepare::SubscriptionPrepareError,
};

pub(crate) struct SubscriptionUpsert {
    pub(crate) source_id: String,
    pub(crate) display_name: String,
    pub(crate) endpoint_url: Zeroizing<Vec<u8>>,
    pub(crate) enabled: bool,
    pub(crate) refresh_interval_seconds: u32,
    pub(crate) expected_revision: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SubscriptionRefreshResult {
    pub(crate) source_id: String,
    pub(crate) revision: u64,
    pub(crate) generation: u64,
    pub(crate) node_count: usize,
    pub(crate) unchanged: bool,
    pub(crate) cleanup_failures: usize,
}

#[derive(Debug, Error)]
pub(crate) enum SubscriptionServiceError {
    #[error("订阅源不存在")]
    SourceNotFound,
    #[error("订阅地址不是 UTF-8 编码")]
    EndpointEncoding,
    #[error(transparent)]
    Fetch(#[from] SubscriptionFetchError),
    #[error(transparent)]
    Prepare(#[from] SubscriptionPrepareError),
    #[error("系统凭据库无法读取订阅地址")]
    CredentialRead,
    #[error("系统凭据库无法完整写入订阅凭据")]
    CredentialWrite(CredentialWriteFailure),
    #[error("订阅数据库提交失败")]
    Commit {
        #[source]
        source: GatewayError,
        cleanup_failures: usize,
    },
    #[error("订阅源修订号已耗尽")]
    RevisionExhausted,
    #[error("订阅刷新任务异常终止")]
    TaskFailed,
    #[error("订阅服务正在关闭")]
    TaskClosed,
    #[error("无法生成订阅刷新标识: {0}")]
    Random(String),
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl SubscriptionServiceError {
    #[must_use]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::SourceNotFound => "NP_SUBSCRIPTION_NOT_FOUND",
            Self::EndpointEncoding => "NP_SUBSCRIPTION_ENDPOINT_INVALID",
            Self::Fetch(error) => error.code(),
            Self::Prepare(error) => error.code(),
            Self::CredentialRead | Self::CredentialWrite(_) => "NP_CREDENTIAL_STORE_FAILED",
            Self::Commit { source, .. } | Self::Gateway(source) => source.code(),
            Self::RevisionExhausted => "NP_SUBSCRIPTION_REVISION_EXHAUSTED",
            Self::TaskFailed => "NP_SUBSCRIPTION_TASK_FAILED",
            Self::TaskClosed => "NP_SUBSCRIPTION_SHUTTING_DOWN",
            Self::Random(_) => "NP_SUBSCRIPTION_RANDOM_FAILED",
            Self::Storage(error) => error.code(),
        }
    }

    #[must_use]
    pub(crate) const fn retryable(&self) -> bool {
        match self {
            Self::Fetch(error) => error.retryable(),
            Self::CredentialRead
            | Self::CredentialWrite(_)
            | Self::Random(_)
            | Self::TaskFailed
            | Self::TaskClosed => true,
            Self::Commit { source, .. } | Self::Gateway(source) => source.retryable(),
            _ => false,
        }
    }

    #[must_use]
    pub(crate) const fn cleanup_failures(&self) -> usize {
        match self {
            Self::CredentialWrite(error) => error.cleanup_failures(),
            Self::Commit {
                cleanup_failures, ..
            } => *cleanup_failures,
            _ => 0,
        }
    }
}
