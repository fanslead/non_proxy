use thiserror::Error;

#[derive(Debug, Error)]
pub enum DnsError {
    #[error("DNS 消息为空或超过长度上限")]
    MessageSize,
    #[error("DNS 查询消息无效")]
    InvalidQuery,
    #[error("DNS 响应消息无效")]
    InvalidResponse,
    #[error("DNS 消息编解码失败")]
    Codec,
    #[error("DNS 域名无效")]
    Domain,
    #[error("DNS 缓存容量无效")]
    CacheCapacity,
    #[error("DNS 缓存锁不可用")]
    CacheLock,
}
