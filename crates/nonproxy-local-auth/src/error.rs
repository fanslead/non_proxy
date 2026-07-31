use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LocalAuthError {
    #[error("本机会话能力文件名无效")]
    FileNameInvalid,
    #[error("本机会话能力目录无效")]
    StateDirectoryInvalid,
    #[error("本机会话能力文件路径无效")]
    CapabilityPathInvalid,
    #[error("本机操作标识无效")]
    OperationIdInvalid,
    #[error("本机会话能力文件操作失败")]
    File(#[source] io::Error),
    #[error("无法生成本机会话能力令牌")]
    Random(#[source] getrandom::Error),
}

impl LocalAuthError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::FileNameInvalid => "NP_LOCAL_AUTH_FILE_NAME_INVALID",
            Self::StateDirectoryInvalid => "NP_LOCAL_AUTH_STATE_DIRECTORY_INVALID",
            Self::CapabilityPathInvalid => "NP_LOCAL_AUTH_CAPABILITY_PATH_INVALID",
            Self::OperationIdInvalid => "NP_LOCAL_AUTH_OPERATION_ID_INVALID",
            Self::File(_) => "NP_LOCAL_AUTH_FILE_FAILED",
            Self::Random(_) => "NP_LOCAL_AUTH_RANDOM_FAILED",
        }
    }
}
