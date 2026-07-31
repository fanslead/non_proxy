use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExitProbeError {
    #[error("出口探针配置无效")]
    Configuration,
    #[error("出口探针随机数生成失败")]
    Random,
    #[error("出口探针连接失败")]
    Connect,
    #[error("出口探针 TLS 验证失败")]
    Tls,
    #[error("出口探针 HTTP 请求失败")]
    Http,
    #[error("出口探针响应状态无效")]
    HttpStatus,
    #[error("出口探针响应超过大小上限")]
    ResponseTooLarge,
    #[error("出口探针响应格式无效")]
    ResponseInvalid,
    #[error("出口探针签名密钥无效")]
    KeyInvalid,
    #[error("出口探针签名无效")]
    SignatureInvalid,
    #[error("出口探针 nonce 不匹配")]
    NonceMismatch,
    #[error("出口探针观测地址不是公网地址")]
    AddressInvalid,
    #[error("出口探针回执时间无效")]
    TimestampInvalid,
    #[error("出口探针请求超时")]
    Timeout,
}
