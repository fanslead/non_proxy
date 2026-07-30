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
    #[error("合成地址空间配置无效")]
    SyntheticAddressSpace,
    #[error("合成地址空间已耗尽")]
    SyntheticAddressExhausted,
    #[error("DNS 查询类型与合成地址族不匹配")]
    SyntheticAddressFamilyMismatch,
}
