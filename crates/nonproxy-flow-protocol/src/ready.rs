use nonproxy_model::OutboundId;

use crate::{FlowProtocolError, WindowUpdate};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlowReady {
    outbound_id: OutboundId,
    initial_window_bytes: u32,
}

impl FlowReady {
    pub fn new(
        outbound_id: OutboundId,
        initial_window_bytes: u32,
    ) -> Result<Self, FlowProtocolError> {
        let _window = WindowUpdate::new(initial_window_bytes)?;
        Ok(Self {
            outbound_id,
            initial_window_bytes,
        })
    }

    #[must_use]
    pub const fn outbound_id(&self) -> &OutboundId {
        &self.outbound_id
    }

    #[must_use]
    pub const fn initial_window_bytes(&self) -> u32 {
        self.initial_window_bytes
    }

    pub fn encode(&self) -> Result<Vec<u8>, FlowProtocolError> {
        let outbound = self.outbound_id.as_str().as_bytes();
        let length = u8::try_from(outbound.len()).map_err(|_| FlowProtocolError::InvalidPayload)?;
        let mut output = Vec::with_capacity(5 + outbound.len());
        output.push(length);
        output.extend_from_slice(outbound);
        output.extend_from_slice(&self.initial_window_bytes.to_be_bytes());
        Ok(output)
    }

    pub fn decode(input: &[u8]) -> Result<Self, FlowProtocolError> {
        let Some(length) = input.first().copied().map(usize::from) else {
            return Err(FlowProtocolError::InvalidPayload);
        };
        let id_end = 1_usize
            .checked_add(length)
            .ok_or(FlowProtocolError::InvalidPayload)?;
        let window_end = id_end
            .checked_add(4)
            .ok_or(FlowProtocolError::InvalidPayload)?;
        if length == 0 || input.len() != window_end {
            return Err(FlowProtocolError::InvalidPayload);
        }
        let outbound = std::str::from_utf8(&input[1..id_end])
            .map_err(|_| FlowProtocolError::InvalidPayload)?;
        let outbound_id = OutboundId::new(outbound).map_err(FlowProtocolError::InvalidOutbound)?;
        let window = u32::from_be_bytes(
            input[id_end..window_end]
                .try_into()
                .map_err(|_| FlowProtocolError::InvalidPayload)?,
        );
        Self::new(outbound_id, window)
    }
}
