use std::fmt;

use nonproxy_model::OutboundId;
use zeroize::{Zeroize, Zeroizing};

use crate::{FlowEndpoint, FlowProtocolError};

pub const CAPABILITY_TOKEN_BYTES: usize = 32;
const MINIMUM_WINDOW_BYTES: u32 = 16 * 1024;
const MAXIMUM_WINDOW_BYTES: u32 = 16 * 1024 * 1024;

pub struct OpenFlowRequest {
    capability: CapabilityToken,
    outbound_id: OutboundId,
    endpoint: FlowEndpoint,
    initial_window_bytes: u32,
}

struct CapabilityToken([u8; CAPABILITY_TOKEN_BYTES]);

impl OpenFlowRequest {
    pub fn new(
        capability: [u8; CAPABILITY_TOKEN_BYTES],
        outbound_id: OutboundId,
        endpoint: FlowEndpoint,
        initial_window_bytes: u32,
    ) -> Result<Self, FlowProtocolError> {
        if !(MINIMUM_WINDOW_BYTES..=MAXIMUM_WINDOW_BYTES).contains(&initial_window_bytes) {
            return Err(FlowProtocolError::InvalidPayload);
        }
        Ok(Self {
            capability: CapabilityToken(capability),
            outbound_id,
            endpoint,
            initial_window_bytes,
        })
    }

    #[must_use]
    pub fn capability(&self) -> &[u8; CAPABILITY_TOKEN_BYTES] {
        &self.capability.0
    }

    #[must_use]
    pub const fn outbound_id(&self) -> &OutboundId {
        &self.outbound_id
    }

    #[must_use]
    pub const fn endpoint(&self) -> &FlowEndpoint {
        &self.endpoint
    }

    #[must_use]
    pub const fn initial_window_bytes(&self) -> u32 {
        self.initial_window_bytes
    }

    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, FlowProtocolError> {
        let outbound = self.outbound_id.as_str().as_bytes();
        let outbound_length =
            u8::try_from(outbound.len()).map_err(|_| FlowProtocolError::InvalidPayload)?;
        let mut output = Zeroizing::new(Vec::with_capacity(64 + outbound.len()));
        output.extend_from_slice(self.capability());
        output.push(outbound_length);
        output.extend_from_slice(outbound);
        self.endpoint.encode_into(&mut output)?;
        output.extend_from_slice(&self.initial_window_bytes.to_be_bytes());
        Ok(output)
    }

    pub fn decode(input: &[u8]) -> Result<Self, FlowProtocolError> {
        if input.len() < CAPABILITY_TOKEN_BYTES + 1 {
            return Err(FlowProtocolError::InvalidPayload);
        }
        let capability = input[..CAPABILITY_TOKEN_BYTES]
            .try_into()
            .map_err(|_| FlowProtocolError::InvalidPayload)?;
        let outbound_length = usize::from(input[CAPABILITY_TOKEN_BYTES]);
        let outbound_start = CAPABILITY_TOKEN_BYTES + 1;
        let endpoint_start = outbound_start
            .checked_add(outbound_length)
            .ok_or(FlowProtocolError::InvalidPayload)?;
        if outbound_length == 0 || input.len() < endpoint_start {
            return Err(FlowProtocolError::InvalidPayload);
        }
        let outbound = std::str::from_utf8(&input[outbound_start..endpoint_start])
            .map_err(|_| FlowProtocolError::InvalidPayload)?;
        let outbound_id = OutboundId::new(outbound).map_err(FlowProtocolError::InvalidOutbound)?;
        let (endpoint, endpoint_length) = FlowEndpoint::decode(&input[endpoint_start..])?;
        let window_start = endpoint_start
            .checked_add(endpoint_length)
            .ok_or(FlowProtocolError::InvalidPayload)?;
        let window_end = window_start
            .checked_add(4)
            .ok_or(FlowProtocolError::InvalidPayload)?;
        if input.len() != window_end {
            return Err(FlowProtocolError::InvalidPayload);
        }
        let initial_window_bytes = u32::from_be_bytes(
            input[window_start..window_end]
                .try_into()
                .map_err(|_| FlowProtocolError::InvalidPayload)?,
        );
        Self::new(capability, outbound_id, endpoint, initial_window_bytes)
    }
}

impl fmt::Debug for OpenFlowRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenFlowRequest")
            .field("capability", &"[REDACTED]")
            .field("outbound_id", &self.outbound_id)
            .field("endpoint", &self.endpoint)
            .field("initial_window_bytes", &self.initial_window_bytes)
            .finish()
    }
}

impl Drop for CapabilityToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
