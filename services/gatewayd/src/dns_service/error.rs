use nonproxy_dns::DnsError;
use thiserror::Error;

use crate::{GatewayError, flow_server::FlowServiceError};

#[derive(Debug, Error)]
pub enum DnsServiceError {
    #[error("DNS 请求无效: {0}")]
    InvalidRequest(&'static str),
    #[error("DNS 查询消息无效: {0}")]
    InvalidQuery(DnsError),
    #[error("DNS 上游响应无效: {0}")]
    InvalidResponse(DnsError),
    #[error("DNS 缓存不可用: {0}")]
    Cache(DnsError),
    #[error("DNS 上游网络不可用")]
    ResolverIo,
    #[error("DNS 上游响应超时")]
    ResolverTimeout,
    #[error("DNS 上游均未返回有效响应")]
    ResolversExhausted,
    #[error("DNS Provider 快照不是当前已激活版本")]
    SnapshotUnavailable,
    #[error("DNS 代理出口不可用: {0}")]
    Proxy(#[from] FlowServiceError),
    #[error("DNS 网关状态不可用: {0}")]
    Gateway(#[from] GatewayError),
}

impl DnsServiceError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) | Self::InvalidQuery(_) => "NP_DNS_REQUEST_INVALID",
            Self::InvalidResponse(_) => "NP_DNS_RESPONSE_INVALID",
            Self::Cache(_) => "NP_DNS_CACHE_UNAVAILABLE",
            Self::ResolverIo | Self::ResolversExhausted => "NP_DNS_RESOLVER_UNAVAILABLE",
            Self::ResolverTimeout => "NP_DNS_RESOLVER_TIMEOUT",
            Self::SnapshotUnavailable => "NP_DNS_SNAPSHOT_UNAVAILABLE",
            Self::Proxy(_) => "NP_DNS_PROXY_UNAVAILABLE",
            Self::Gateway(_) => "NP_DNS_GATEWAY_UNAVAILABLE",
        }
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Cache(_)
                | Self::ResolverIo
                | Self::ResolverTimeout
                | Self::ResolversExhausted
                | Self::SnapshotUnavailable
                | Self::Proxy(_)
                | Self::Gateway(_)
        )
    }

    #[must_use]
    pub const fn is_invalid_argument(&self) -> bool {
        matches!(self, Self::InvalidRequest(_) | Self::InvalidQuery(_))
    }
}
