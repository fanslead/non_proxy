use thiserror::Error;

#[derive(Debug, Error)]
pub enum WindowsWfpError {
    #[error("{operation}失败，Windows 错误码 {code:#010x}")]
    Windows { operation: &'static str, code: u32 },
    #[error("{0}")]
    InvalidData(&'static str),
    #[error("WFP redirect 数据超过安全上限")]
    RedirectDataTooLarge,
}

impl WindowsWfpError {
    #[cfg(windows)]
    pub(crate) const fn windows(operation: &'static str, code: u32) -> Self {
        Self::Windows { operation, code }
    }
}
