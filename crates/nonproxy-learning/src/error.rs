use thiserror::Error;

#[derive(Debug, Error)]
pub enum LearningError {
    #[error("学习领域输入无效: {0}")]
    Model(#[from] nonproxy_model::ModelError),
    #[error("学习标识无效")]
    InvalidIdentifier,
    #[error("学习时长必须在 5 秒到 5 分钟之间")]
    InvalidDuration,
    #[error("学习会话时间范围无效")]
    InvalidTimeRange,
    #[error("网站学习必须绑定浏览器标签页上下文")]
    BrowserContextRequired,
    #[error("应用学习不能携带浏览器标签页上下文")]
    BrowserContextNotAllowed,
    #[error("观测所属的浏览器标签页上下文与会话不一致")]
    BrowserContextMismatch,
    #[error("学习会话当前不可接收观测")]
    SessionNotActive,
}

impl LearningError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Model(_) => "NP_LEARNING_MODEL_INVALID",
            Self::InvalidIdentifier => "NP_LEARNING_IDENTIFIER_INVALID",
            Self::InvalidDuration => "NP_LEARNING_DURATION_INVALID",
            Self::InvalidTimeRange => "NP_LEARNING_TIME_RANGE_INVALID",
            Self::BrowserContextRequired => "NP_LEARNING_BROWSER_CONTEXT_REQUIRED",
            Self::BrowserContextNotAllowed => "NP_LEARNING_BROWSER_CONTEXT_NOT_ALLOWED",
            Self::BrowserContextMismatch => "NP_LEARNING_BROWSER_CONTEXT_MISMATCH",
            Self::SessionNotActive => "NP_LEARNING_SESSION_NOT_ACTIVE",
        }
    }
}
