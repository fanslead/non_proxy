use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProbeServerError {
    #[error("服务配置无效")]
    Configuration,
    #[error("服务文件不可用")]
    File,
    #[error("TLS 配置无效")]
    Tls,
    #[error("服务网络失败")]
    Io(#[from] std::io::Error),
    #[error("探针签名密钥无效")]
    SigningKey,
}
