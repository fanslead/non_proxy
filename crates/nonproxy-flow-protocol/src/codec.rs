use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    FRAME_HEADER_BYTES, FRAME_MAGIC, FRAME_VERSION, FlowFrame, FlowId, FlowProtocolError,
    FrameType, MAX_FRAME_PAYLOAD_BYTES,
};

pub async fn read_frame(
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<FlowFrame, FlowProtocolError> {
    let mut header = [0_u8; FRAME_HEADER_BYTES];
    reader.read_exact(&mut header).await?;
    if header[0..4] != FRAME_MAGIC {
        return Err(FlowProtocolError::InvalidMagic);
    }
    let version = u16::from_be_bytes([header[4], header[5]]);
    if version != FRAME_VERSION {
        return Err(FlowProtocolError::UnsupportedVersion);
    }
    let frame_type = FrameType::try_from(header[6])?;
    let flags = header[7];
    let mut flow_id = [0_u8; 16];
    flow_id.copy_from_slice(&header[8..24]);
    let flow_id = FlowId::new(flow_id)?;
    let sequence = u64::from_be_bytes(
        header[24..32]
            .try_into()
            .map_err(|_| FlowProtocolError::InvalidPayload)?,
    );
    let payload_length = u32::from_be_bytes(
        header[32..36]
            .try_into()
            .map_err(|_| FlowProtocolError::InvalidPayload)?,
    ) as usize;
    if payload_length > MAX_FRAME_PAYLOAD_BYTES {
        return Err(FlowProtocolError::PayloadTooLarge);
    }
    let mut payload = vec![0_u8; payload_length];
    reader.read_exact(&mut payload).await?;
    FlowFrame::new(frame_type, flags, flow_id, sequence, payload)
}

pub async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    frame: &FlowFrame,
) -> Result<(), FlowProtocolError> {
    let payload_length =
        u32::try_from(frame.payload().len()).map_err(|_| FlowProtocolError::PayloadTooLarge)?;
    let mut header = [0_u8; FRAME_HEADER_BYTES];
    header[0..4].copy_from_slice(&FRAME_MAGIC);
    header[4..6].copy_from_slice(&FRAME_VERSION.to_be_bytes());
    header[6] = frame.frame_type() as u8;
    header[7] = frame.flags();
    header[8..24].copy_from_slice(frame.flow_id().as_bytes());
    header[24..32].copy_from_slice(&frame.sequence().to_be_bytes());
    header[32..36].copy_from_slice(&payload_length.to_be_bytes());
    writer.write_all(&header).await?;
    writer.write_all(frame.payload()).await?;
    writer.flush().await?;
    Ok(())
}
