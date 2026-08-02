use thiserror::Error;

#[derive(Debug, Error)]
pub enum FlowProtocolError {
    #[error("数据面帧魔数无效")]
    InvalidMagic,
    #[error("数据面帧版本不受支持")]
    UnsupportedVersion,
    #[error("数据面帧类型无效")]
    InvalidFrameType,
    #[error("数据面帧标志无效")]
    InvalidFlags,
    #[error("数据面帧载荷超过上限")]
    PayloadTooLarge,
    #[error("数据面帧载荷结构无效")]
    InvalidPayload,
    #[error("数据面 flow 标识不能全为零")]
    InvalidFlowId,
    #[error("数据面帧序列不连续")]
    SequenceMismatch,
    #[error("数据面帧序列已耗尽")]
    SequenceExhausted,
    #[error("数据面 endpoint 无效: {0}")]
    InvalidEndpoint(#[from] nonproxy_model::ModelError),
    #[error("数据面出口标识无效: {0}")]
    InvalidOutbound(#[source] nonproxy_model::ModelError),
    #[error("数据面出口组标识无效: {0}")]
    InvalidOutboundGroup(#[source] nonproxy_model::ModelError),
    #[error("数据面读写失败: {0}")]
    Io(#[from] std::io::Error),
}
