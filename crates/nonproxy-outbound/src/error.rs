use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutboundError {
    #[error("代理 endpoint 无效")]
    InvalidEndpoint,
    #[error("代理凭据格式无效")]
    InvalidCredential,
    #[error("代理连接超时")]
    ConnectTimeout,
    #[error("HTTP CONNECT 响应头超过上限")]
    HttpHeaderTooLarge,
    #[error("HTTP CONNECT 响应无效")]
    InvalidHttpResponse,
    #[error("HTTP CONNECT 被代理服务器拒绝，状态码 {0}")]
    HttpRejected(u16),
    #[error("当前代理出口不支持 UDP")]
    UdpUnsupported,
    #[error("SOCKS5 响应无效")]
    InvalidSocksResponse,
    #[error("SOCKS5 认证失败")]
    SocksAuthenticationFailed,
    #[error("SOCKS5 UDP 分片暂不受支持")]
    SocksUdpFragmentUnsupported,
    #[error("SOCKS5 握手失败: {0}")]
    Socks(#[from] tokio_socks::Error),
    #[error("代理网络读写失败: {0}")]
    Io(#[from] std::io::Error),
}
