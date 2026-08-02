use nonproxy_flow_protocol::FlowProtocolError;
use nonproxy_outbound::OutboundError;
use thiserror::Error;

use crate::{GatewayError, credential_store::CredentialStoreError};

#[derive(Debug, Error)]
pub enum FlowServiceError {
    #[error("数据面 I/O 失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("数据面帧无效: {0}")]
    Protocol(#[from] FlowProtocolError),
    #[error("Provider 数据面认证失败")]
    Authentication,
    #[error("防回环系统快照尚未激活")]
    SystemSnapshotPending,
    #[error("代理出口不存在")]
    OutboundNotFound,
    #[error("Provider 策略快照不是当前已激活版本")]
    PolicySnapshotUnavailable,
    #[error("策略快照中的代理出口组不存在")]
    OutboundGroupNotFound,
    #[error("代理出口组没有已确认健康的成员")]
    OutboundGroupUnavailable,
    #[error("代理出口未启用")]
    OutboundDisabled,
    #[error("代理出口类型暂不支持")]
    OutboundUnsupported,
    #[error("代理出口配置不完整")]
    OutboundInvalid,
    #[error("代理凭据不可用: {0}")]
    Credential(#[from] CredentialStoreError),
    #[error("代理凭据后台任务失败")]
    CredentialTask,
    #[error("代理连接失败: {0}")]
    Outbound(#[from] OutboundError),
    #[error("代理加密路径认证失败")]
    OutboundAuthentication,
    #[error("网关状态读取失败: {0}")]
    Gateway(#[from] GatewayError),
    #[error("数据面发送通道已关闭")]
    ChannelClosed,
    #[error("数据面写入任务失败")]
    WriterTask,
    #[error("对端已关闭数据面 flow")]
    PeerClosed,
    #[error("数据面窗口无效")]
    InvalidWindow,
}

impl FlowServiceError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "NP_FLOW_IO_FAILED",
            Self::Protocol(_) => "NP_FLOW_PROTOCOL_INVALID",
            Self::Authentication => "NP_FLOW_AUTHENTICATION_FAILED",
            Self::SystemSnapshotPending => "NP_FLOW_SYSTEM_SNAPSHOT_PENDING",
            Self::OutboundNotFound => "NP_FLOW_OUTBOUND_NOT_FOUND",
            Self::PolicySnapshotUnavailable => "NP_FLOW_POLICY_SNAPSHOT_UNAVAILABLE",
            Self::OutboundGroupNotFound => "NP_FLOW_OUTBOUND_GROUP_NOT_FOUND",
            Self::OutboundGroupUnavailable => "NP_FLOW_OUTBOUND_GROUP_UNAVAILABLE",
            Self::OutboundDisabled => "NP_FLOW_OUTBOUND_DISABLED",
            Self::OutboundUnsupported => "NP_FLOW_OUTBOUND_UNSUPPORTED",
            Self::OutboundInvalid => "NP_FLOW_OUTBOUND_INVALID",
            Self::Credential(_) | Self::CredentialTask => "NP_FLOW_CREDENTIAL_UNAVAILABLE",
            Self::Outbound(_) => "NP_FLOW_OUTBOUND_CONNECT_FAILED",
            Self::OutboundAuthentication => "NP_FLOW_OUTBOUND_AUTHENTICATION_FAILED",
            Self::Gateway(_) => "NP_FLOW_GATEWAY_UNAVAILABLE",
            Self::ChannelClosed | Self::WriterTask => "NP_FLOW_CHANNEL_CLOSED",
            Self::PeerClosed => "NP_FLOW_PEER_CLOSED",
            Self::InvalidWindow => "NP_FLOW_WINDOW_INVALID",
        }
    }
}
