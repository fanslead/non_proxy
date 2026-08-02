use nonproxy_learning::LearningError;
use nonproxy_local_auth::LocalAuthError;
use nonproxy_model::ModelError;
use nonproxy_policy_compiler::CompileError;
use nonproxy_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("领域数据无效: {0}")]
    Model(#[from] ModelError),
    #[error("学习领域数据无效: {0}")]
    Learning(#[from] LearningError),
    #[error("持久化操作失败: {0}")]
    Storage(#[from] StorageError),
    #[error("策略编译失败: {0}")]
    Compile(#[from] CompileError),
    #[error("快照编码失败: {0}")]
    SnapshotEncoding(#[from] prost::EncodeError),
    #[error("快照解码失败: {0}")]
    SnapshotDecoding(#[from] prost::DecodeError),
    #[error("输入契约无效: {0}")]
    InvalidContract(&'static str),
    #[error("请求超出允许范围: {0}")]
    InvalidRequest(&'static str),
    #[error("默认代理尚未通过当前配置的新鲜握手测试")]
    DefaultOutboundUnverified,
    #[error("当前没有可取消的运行态覆盖")]
    RuntimeOverrideNotActive,
    #[error("运行态覆盖时长必须在 1 秒到 1 小时之间")]
    RuntimeOverrideDurationInvalid,
    #[error("运行态覆盖需要已有活动策略快照")]
    RuntimeOverrideActiveSnapshotMissing,
    #[error("{0}状态互斥锁已损坏")]
    StateLockPoisoned(&'static str),
    #[error("后台数据库任务失败: {0}")]
    DatabaseTask(String),
    #[error("系统时间早于 Unix epoch")]
    ClockBeforeUnixEpoch,
    #[error("系统时间超出可表示范围")]
    ClockOverflow,
    #[error("快照版本已耗尽")]
    SnapshotVersionExhausted,
    #[error("本地路径无效: {0}")]
    InvalidLocalPath(&'static str),
    #[error("本地文件操作失败: {operation}: {source}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("无法生成本机会话能力令牌: {0}")]
    Random(String),
    #[error("本机会话认证失败: {0}")]
    LocalAuth(#[from] LocalAuthError),
    #[error("后台运行身份无效: {0}")]
    RuntimeIdentity(String),
    #[error("RPC 服务失败: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("Windows 数据面失败: {0}")]
    WindowsDataPlane(String),
}

impl GatewayError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Model(
                ModelError::InvalidNetworkFingerprint
                | ModelError::InvalidNetworkProfileDisplayName
                | ModelError::InvalidNetworkProfileRevision,
            ) => "NP_NETWORK_PROFILE_INVALID",
            Self::Model(_) | Self::InvalidContract(_) | Self::InvalidRequest(_) => {
                "NP_REQUEST_INVALID"
            }
            Self::Learning(error) => error.code(),
            Self::LocalAuth(LocalAuthError::Random(_)) => "NP_SESSION_TOKEN_FAILED",
            Self::LocalAuth(_) => "NP_LOCAL_PATH_INVALID",
            Self::DefaultOutboundUnverified => "NP_DEFAULT_OUTBOUND_UNVERIFIED",
            Self::RuntimeOverrideNotActive => "NP_RUNTIME_OVERRIDE_NOT_ACTIVE",
            Self::RuntimeOverrideDurationInvalid => "NP_RUNTIME_OVERRIDE_DURATION_INVALID",
            Self::RuntimeOverrideActiveSnapshotMissing => {
                "NP_RUNTIME_OVERRIDE_ACTIVE_SNAPSHOT_MISSING"
            }
            Self::Storage(StorageError::PolicyRevisionConflict) => "NP_POLICY_REVISION_CONFLICT",
            Self::Storage(StorageError::OutboundRevisionConflict) => {
                "NP_OUTBOUND_REVISION_CONFLICT"
            }
            Self::Storage(
                error @ (StorageError::OutboundGroupInvalid
                | StorageError::OutboundGroupRevisionConflict
                | StorageError::OutboundGroupMemberNotFound
                | StorageError::OutboundGroupMemberUnsupported),
            ) => error.code(),
            Self::Storage(
                error @ (StorageError::SubscriptionInvalid
                | StorageError::SubscriptionRevisionConflict
                | StorageError::SubscriptionGenerationConflict
                | StorageError::SubscriptionOwnershipConflict
                | StorageError::SubscriptionDefaultOutboundRemoved
                | StorageError::SubscriptionOutboundInUse),
            ) => error.code(),
            Self::Storage(StorageError::NetworkProfileRevisionConflict) => {
                "NP_NETWORK_PROFILE_REVISION_CONFLICT"
            }
            Self::Storage(StorageError::NetworkProfileFingerprintConflict) => {
                "NP_NETWORK_PROFILE_FINGERPRINT_CONFLICT"
            }
            Self::Storage(StorageError::NetworkProfileInUse) => "NP_NETWORK_PROFILE_IN_USE",
            Self::Storage(StorageError::RoutingRevisionConflict) => "NP_ROUTING_REVISION_CONFLICT",
            Self::Storage(StorageError::DefaultOutboundUnavailable) => {
                "NP_DEFAULT_OUTBOUND_UNAVAILABLE"
            }
            Self::Storage(
                error @ (StorageError::ConnectionDecisionInvalid
                | StorageError::DecisionEvidenceInvalid
                | StorageError::ConnectionDecisionReplayMismatch),
            ) => error.code(),
            Self::Storage(StorageError::PendingSnapshotExists) => "NP_SNAPSHOT_ALREADY_PENDING",
            Self::Storage(StorageError::ActiveSnapshotVersionConflict) => {
                "NP_SNAPSHOT_ACTIVE_VERSION_CONFLICT"
            }
            Self::Storage(
                error @ (StorageError::LearningSessionNotFound
                | StorageError::ActiveLearningSessionExists
                | StorageError::LearningCandidateLimitReached
                | StorageError::LearningObservationLimitReached
                | StorageError::LearningSessionStillActive
                | StorageError::LearningConfirmationInvalid
                | StorageError::LearningSessionAlreadyConfirmed
                | StorageError::LearningConfirmationReplayMismatch
                | StorageError::Learning(_)),
            ) => error.code(),
            Self::Storage(_) => "NP_STORAGE_FAILURE",
            Self::Compile(_) => "NP_POLICY_COMPILE_REJECTED",
            Self::SnapshotEncoding(_) | Self::SnapshotDecoding(_) => "NP_SNAPSHOT_CODEC_FAILED",
            Self::StateLockPoisoned(_) | Self::DatabaseTask(_) => "NP_STATE_UNAVAILABLE",
            Self::ClockBeforeUnixEpoch | Self::ClockOverflow => "NP_CLOCK_INVALID",
            Self::SnapshotVersionExhausted => "NP_SNAPSHOT_VERSION_EXHAUSTED",
            Self::InvalidLocalPath(_) | Self::Io { .. } => "NP_LOCAL_PATH_INVALID",
            Self::Random(_) => "NP_SESSION_TOKEN_FAILED",
            Self::RuntimeIdentity(_) => "NP_RUNTIME_IDENTITY_INVALID",
            Self::Transport(_) => "NP_CONTROL_TRANSPORT_FAILED",
            Self::WindowsDataPlane(_) => "NP_WINDOWS_DATA_PLANE_FAILED",
        }
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Storage(StorageError::WriteLeaseUnavailable { .. })
                | Self::Storage(StorageError::PendingSnapshotExists)
                | Self::DatabaseTask(_)
                | Self::Transport(_)
                | Self::WindowsDataPlane(_)
        )
    }
}
