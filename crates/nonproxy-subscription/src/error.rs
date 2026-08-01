use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubscriptionFetchError {
    #[error("订阅地址无效")]
    EndpointInvalid,
    #[error("订阅地址没有解析到可用的公网地址")]
    AddressNotPublic,
    #[error("订阅地址解析失败")]
    Resolve,
    #[error("订阅服务器连接失败")]
    Connect,
    #[error("订阅服务器身份验证失败")]
    Tls,
    #[error("订阅 HTTP 请求失败")]
    Http,
    #[error("订阅服务器返回了不受支持的状态")]
    HttpStatus,
    #[error("订阅响应使用了不受支持的内容编码")]
    ContentEncoding,
    #[error("订阅响应超过大小上限")]
    ResponseTooLarge,
    #[error("订阅请求超时")]
    Timeout,
}

impl SubscriptionFetchError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EndpointInvalid => "NP_SUBSCRIPTION_ENDPOINT_INVALID",
            Self::AddressNotPublic => "NP_SUBSCRIPTION_ADDRESS_NOT_PUBLIC",
            Self::Resolve => "NP_SUBSCRIPTION_RESOLVE_FAILED",
            Self::Connect => "NP_SUBSCRIPTION_CONNECT_FAILED",
            Self::Tls => "NP_SUBSCRIPTION_TLS_FAILED",
            Self::Http => "NP_SUBSCRIPTION_HTTP_FAILED",
            Self::HttpStatus => "NP_SUBSCRIPTION_HTTP_STATUS_INVALID",
            Self::ContentEncoding => "NP_SUBSCRIPTION_CONTENT_ENCODING_UNSUPPORTED",
            Self::ResponseTooLarge => "NP_SUBSCRIPTION_RESPONSE_TOO_LARGE",
            Self::Timeout => "NP_SUBSCRIPTION_TIMEOUT",
        }
    }

    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Resolve | Self::Connect | Self::Http | Self::HttpStatus | Self::Timeout
        )
    }
}
