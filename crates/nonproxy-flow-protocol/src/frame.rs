use std::fmt;

use zeroize::Zeroizing;

use crate::FlowProtocolError;

pub const FRAME_MAGIC: [u8; 4] = *b"NPF1";
pub const FRAME_VERSION: u16 = 1;
pub const FRAME_HEADER_BYTES: usize = 36;
pub const MAX_FRAME_PAYLOAD_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameType {
    OpenTcp = 1,
    OpenUdp = 2,
    Data = 3,
    Datagram = 4,
    HalfClose = 5,
    Close = 6,
    WindowUpdate = 7,
    Error = 8,
    Ping = 9,
    Pong = 10,
}

impl TryFrom<u8> for FrameType {
    type Error = FlowProtocolError;

    fn try_from(value: u8) -> Result<Self, FlowProtocolError> {
        match value {
            1 => Ok(Self::OpenTcp),
            2 => Ok(Self::OpenUdp),
            3 => Ok(Self::Data),
            4 => Ok(Self::Datagram),
            5 => Ok(Self::HalfClose),
            6 => Ok(Self::Close),
            7 => Ok(Self::WindowUpdate),
            8 => Ok(Self::Error),
            9 => Ok(Self::Ping),
            10 => Ok(Self::Pong),
            _ => Err(FlowProtocolError::InvalidFrameType),
        }
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct FlowId([u8; 16]);

impl FlowId {
    pub fn new(bytes: [u8; 16]) -> Result<Self, FlowProtocolError> {
        if bytes == [0; 16] {
            return Err(FlowProtocolError::InvalidFlowId);
        }
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Debug for FlowId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

pub struct FlowFrame {
    frame_type: FrameType,
    flags: u8,
    flow_id: FlowId,
    sequence: u64,
    payload: StoredPayload,
}

enum StoredPayload {
    Regular(Vec<u8>),
    Sensitive(Zeroizing<Vec<u8>>),
}

impl FlowFrame {
    pub fn new(
        frame_type: FrameType,
        flags: u8,
        flow_id: FlowId,
        sequence: u64,
        payload: Vec<u8>,
    ) -> Result<Self, FlowProtocolError> {
        if flags != 0 {
            return Err(FlowProtocolError::InvalidFlags);
        }
        if payload.len() > MAX_FRAME_PAYLOAD_BYTES {
            return Err(FlowProtocolError::PayloadTooLarge);
        }
        validate_empty_payload(frame_type, &payload)?;
        let payload = match frame_type {
            FrameType::OpenTcp | FrameType::OpenUdp => {
                StoredPayload::Sensitive(Zeroizing::new(payload))
            }
            _ => StoredPayload::Regular(payload),
        };
        Ok(Self {
            frame_type,
            flags,
            flow_id,
            sequence,
            payload,
        })
    }

    #[must_use]
    pub const fn frame_type(&self) -> FrameType {
        self.frame_type
    }

    #[must_use]
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    #[must_use]
    pub const fn flow_id(&self) -> FlowId {
        self.flow_id
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        match &self.payload {
            StoredPayload::Regular(value) => value,
            StoredPayload::Sensitive(value) => value.as_slice(),
        }
    }
}

impl fmt::Debug for FlowFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FlowFrame")
            .field("frame_type", &self.frame_type)
            .field("flags", &self.flags)
            .field("flow_id", &self.flow_id)
            .field("sequence", &self.sequence)
            .field("payload_length", &self.payload().len())
            .finish()
    }
}

fn validate_empty_payload(frame_type: FrameType, payload: &[u8]) -> Result<(), FlowProtocolError> {
    if matches!(
        frame_type,
        FrameType::HalfClose | FrameType::Close | FrameType::Ping | FrameType::Pong
    ) && !payload.is_empty()
    {
        return Err(FlowProtocolError::InvalidPayload);
    }
    Ok(())
}
