use std::{io, path::PathBuf};

use nonproxy_learning::LearningError;
use nonproxy_model::ModelError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite 操作失败")]
    Sqlite(#[from] rusqlite::Error),
    #[error("文件操作失败: {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("领域数据校验失败")]
    Model(#[from] ModelError),
    #[error("学习领域数据校验失败")]
    Learning(#[from] LearningError),
    #[error("数据库父目录不存在: {0}")]
    ParentDirectoryMissing(PathBuf),
    #[error("数据库或写锁路径不能是符号链接: {0}")]
    SymlinkPathRejected(PathBuf),
    #[error("数据库父目录允许组或其他用户访问: {0}")]
    InsecureParentPermissions(PathBuf),
    #[error("另一个写入进程已经持有数据库租约")]
    WriteLeaseUnavailable {
        #[source]
        source: io::Error,
    },
    #[error("现有数据库没有受支持的迁移历史，拒绝接管")]
    UnmanagedDatabase,
    #[error("数据库包含未知迁移版本: {0}")]
    UnknownMigration(i64),
    #[error("已应用迁移内容或名称发生变化: {0}")]
    MigrationDiverged(i64),
    #[error("迁移版本不连续: 期望 {expected}，实际 {actual}")]
    MigrationSequence { expected: i64, actual: i64 },
    #[error("数据库完整性检查失败: {0}")]
    IntegrityCheck(String),
    #[error("策略修订冲突")]
    PolicyRevisionConflict,
    #[error("出口配置修订冲突")]
    OutboundRevisionConflict,
    #[error("出口配置无效")]
    OutboundInvalid,
    #[error("默认路由配置修订冲突")]
    RoutingRevisionConflict,
    #[error("默认代理出口不存在、未启用或能力不足")]
    DefaultOutboundUnavailable,
    #[error("连接决策记录无效")]
    ConnectionDecisionInvalid,
    #[error("连接决策证据与动作不一致")]
    DecisionEvidenceInvalid,
    #[error("连接决策幂等重放内容不一致")]
    ConnectionDecisionReplayMismatch,
    #[error("出口探针回执无效")]
    ExitProbeInvalid,
    #[error("出口探针回执幂等重放内容不一致")]
    ExitProbeReplayMismatch,
    #[error("凭据引用无效")]
    CredentialReferenceInvalid,
    #[error("订阅源配置无效")]
    SubscriptionInvalid,
    #[error("订阅源修订冲突")]
    SubscriptionRevisionConflict,
    #[error("订阅内容代数已经变化")]
    SubscriptionGenerationConflict,
    #[error("订阅节点归属冲突")]
    SubscriptionOwnershipConflict,
    #[error("默认代理出口仍依赖已从订阅移除的节点")]
    SubscriptionDefaultOutboundRemoved,
    #[error("网络画像修订冲突")]
    NetworkProfileRevisionConflict,
    #[error("网络画像指纹已被其他配置档使用")]
    NetworkProfileFingerprintConflict,
    #[error("网络画像仍被策略引用")]
    NetworkProfileInUse,
    #[error("存储数据损坏或无法识别: {field}")]
    CorruptData { field: &'static str },
    #[error("策略快照版本必须单调递增")]
    SnapshotVersionNotMonotonic,
    #[error("已有待发布策略快照")]
    PendingSnapshotExists,
    #[error("策略快照不存在")]
    SnapshotNotFound,
    #[error("策略快照状态不允许当前操作")]
    SnapshotStateConflict,
    #[error("当前活动策略快照已变化")]
    ActiveSnapshotVersionConflict,
    #[error("策略快照内容哈希不匹配")]
    SnapshotHashMismatch,
    #[error("策略快照载荷无效")]
    SnapshotPayloadInvalid,
    #[error("Provider ACK 标识无效")]
    ProviderIdInvalid,
    #[error("Provider ACK 集合为空")]
    RequiredProvidersEmpty,
    #[error("Provider 尚未全部确认策略快照")]
    ProviderAcknowledgementMissing,
    #[error("错误码格式无效")]
    ErrorCodeInvalid,
    #[error("日志保留参数无效")]
    RetentionInvalid,
    #[error("学习会话不存在")]
    LearningSessionNotFound,
    #[error("相同目标已有活动学习会话")]
    ActiveLearningSessionExists,
    #[error("学习会话已达到候选数量上限")]
    LearningCandidateLimitReached,
    #[error("学习会话已达到观测数量上限")]
    LearningObservationLimitReached,
    #[error("学习会话仍在进行，不能确认候选")]
    LearningSessionStillActive,
    #[error("学习候选确认内容无效")]
    LearningConfirmationInvalid,
    #[error("学习会话已经确认")]
    LearningSessionAlreadyConfirmed,
    #[error("学习确认幂等请求与既有收据不一致")]
    LearningConfirmationReplayMismatch,
    #[error("合成 DNS 配置无效")]
    SyntheticDnsConfigInvalid,
    #[error("合成 DNS 绑定数量无效")]
    SyntheticDnsLimitInvalid,
    #[error("合成 DNS 地址空间已耗尽")]
    SyntheticDnsAddressExhausted,
}

impl StorageError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Sqlite(_) => "NP_STORAGE_SQLITE_FAILED",
            Self::Io { .. } => "NP_STORAGE_IO_FAILED",
            Self::Model(_) => "NP_STORAGE_MODEL_INVALID",
            Self::Learning(error) => error.code(),
            Self::ParentDirectoryMissing(_) => "NP_STORAGE_PARENT_MISSING",
            Self::SymlinkPathRejected(_) => "NP_STORAGE_SYMLINK_REJECTED",
            Self::InsecureParentPermissions(_) => "NP_STORAGE_PARENT_PERMISSIONS_INSECURE",
            Self::WriteLeaseUnavailable { .. } => "NP_STORAGE_WRITER_ALREADY_ACTIVE",
            Self::UnmanagedDatabase => "NP_STORAGE_UNMANAGED_DATABASE",
            Self::UnknownMigration(_) => "NP_STORAGE_MIGRATION_UNKNOWN",
            Self::MigrationDiverged(_) => "NP_STORAGE_MIGRATION_DIVERGED",
            Self::MigrationSequence { .. } => "NP_STORAGE_MIGRATION_SEQUENCE_INVALID",
            Self::IntegrityCheck(_) => "NP_STORAGE_INTEGRITY_FAILED",
            Self::PolicyRevisionConflict => "NP_STORAGE_POLICY_REVISION_CONFLICT",
            Self::OutboundRevisionConflict => "NP_STORAGE_OUTBOUND_REVISION_CONFLICT",
            Self::OutboundInvalid => "NP_STORAGE_OUTBOUND_INVALID",
            Self::RoutingRevisionConflict => "NP_STORAGE_ROUTING_REVISION_CONFLICT",
            Self::DefaultOutboundUnavailable => "NP_STORAGE_DEFAULT_OUTBOUND_UNAVAILABLE",
            Self::ConnectionDecisionInvalid => "NP_STORAGE_CONNECTION_DECISION_INVALID",
            Self::DecisionEvidenceInvalid => "NP_STORAGE_DECISION_EVIDENCE_INVALID",
            Self::ConnectionDecisionReplayMismatch => {
                "NP_STORAGE_CONNECTION_DECISION_REPLAY_MISMATCH"
            }
            Self::ExitProbeInvalid => "NP_STORAGE_EXIT_PROBE_INVALID",
            Self::ExitProbeReplayMismatch => "NP_STORAGE_EXIT_PROBE_REPLAY_MISMATCH",
            Self::CredentialReferenceInvalid => "NP_STORAGE_CREDENTIAL_REFERENCE_INVALID",
            Self::SubscriptionInvalid => "NP_STORAGE_SUBSCRIPTION_INVALID",
            Self::SubscriptionRevisionConflict => "NP_STORAGE_SUBSCRIPTION_REVISION_CONFLICT",
            Self::SubscriptionGenerationConflict => "NP_STORAGE_SUBSCRIPTION_GENERATION_CONFLICT",
            Self::SubscriptionOwnershipConflict => "NP_STORAGE_SUBSCRIPTION_OWNERSHIP_CONFLICT",
            Self::SubscriptionDefaultOutboundRemoved => {
                "NP_STORAGE_SUBSCRIPTION_DEFAULT_OUTBOUND_REMOVED"
            }
            Self::NetworkProfileRevisionConflict => "NP_STORAGE_NETWORK_PROFILE_REVISION_CONFLICT",
            Self::NetworkProfileFingerprintConflict => {
                "NP_STORAGE_NETWORK_PROFILE_FINGERPRINT_CONFLICT"
            }
            Self::NetworkProfileInUse => "NP_STORAGE_NETWORK_PROFILE_IN_USE",
            Self::CorruptData { .. } => "NP_STORAGE_CORRUPT_DATA",
            Self::SnapshotVersionNotMonotonic => "NP_STORAGE_SNAPSHOT_VERSION_NOT_MONOTONIC",
            Self::PendingSnapshotExists => "NP_STORAGE_SNAPSHOT_PENDING_EXISTS",
            Self::SnapshotNotFound => "NP_STORAGE_SNAPSHOT_NOT_FOUND",
            Self::SnapshotStateConflict => "NP_STORAGE_SNAPSHOT_STATE_CONFLICT",
            Self::ActiveSnapshotVersionConflict => "NP_STORAGE_ACTIVE_SNAPSHOT_VERSION_CONFLICT",
            Self::SnapshotHashMismatch => "NP_STORAGE_SNAPSHOT_HASH_MISMATCH",
            Self::SnapshotPayloadInvalid => "NP_STORAGE_SNAPSHOT_PAYLOAD_INVALID",
            Self::ProviderIdInvalid => "NP_STORAGE_PROVIDER_ID_INVALID",
            Self::RequiredProvidersEmpty => "NP_STORAGE_REQUIRED_PROVIDERS_EMPTY",
            Self::ProviderAcknowledgementMissing => "NP_STORAGE_PROVIDER_ACK_MISSING",
            Self::ErrorCodeInvalid => "NP_STORAGE_ERROR_CODE_INVALID",
            Self::RetentionInvalid => "NP_STORAGE_RETENTION_INVALID",
            Self::LearningSessionNotFound => "NP_LEARNING_SESSION_NOT_FOUND",
            Self::ActiveLearningSessionExists => "NP_LEARNING_SESSION_ALREADY_ACTIVE",
            Self::LearningCandidateLimitReached => "NP_LEARNING_CANDIDATE_LIMIT_REACHED",
            Self::LearningObservationLimitReached => "NP_LEARNING_OBSERVATION_LIMIT_REACHED",
            Self::LearningSessionStillActive => "NP_LEARNING_SESSION_STILL_ACTIVE",
            Self::LearningConfirmationInvalid => "NP_LEARNING_CONFIRMATION_INVALID",
            Self::LearningSessionAlreadyConfirmed => "NP_LEARNING_SESSION_ALREADY_CONFIRMED",
            Self::LearningConfirmationReplayMismatch => "NP_LEARNING_CONFIRMATION_REPLAY_MISMATCH",
            Self::SyntheticDnsConfigInvalid => "NP_STORAGE_SYNTHETIC_DNS_CONFIG_INVALID",
            Self::SyntheticDnsLimitInvalid => "NP_STORAGE_SYNTHETIC_DNS_LIMIT_INVALID",
            Self::SyntheticDnsAddressExhausted => "NP_STORAGE_SYNTHETIC_DNS_EXHAUSTED",
        }
    }
}
