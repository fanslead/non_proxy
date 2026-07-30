use crate::FlowProtocolError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowUpdate {
    bytes: u32,
}

impl WindowUpdate {
    pub fn new(bytes: u32) -> Result<Self, FlowProtocolError> {
        if bytes == 0 {
            return Err(FlowProtocolError::InvalidPayload);
        }
        Ok(Self { bytes })
    }

    #[must_use]
    pub const fn bytes(self) -> u32 {
        self.bytes
    }

    #[must_use]
    pub const fn encode(self) -> [u8; 4] {
        self.bytes.to_be_bytes()
    }

    pub fn decode(input: &[u8]) -> Result<Self, FlowProtocolError> {
        let bytes = u32::from_be_bytes(
            input
                .try_into()
                .map_err(|_| FlowProtocolError::InvalidPayload)?,
        );
        Self::new(bytes)
    }
}
