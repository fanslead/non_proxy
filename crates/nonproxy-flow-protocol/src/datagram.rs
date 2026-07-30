use crate::{FlowEndpoint, FlowProtocolError};

pub const MAX_DATAGRAM_BYTES: usize = 65_535;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatagramPayload {
    endpoint: FlowEndpoint,
    content: Vec<u8>,
}

impl DatagramPayload {
    pub fn new(endpoint: FlowEndpoint, content: Vec<u8>) -> Result<Self, FlowProtocolError> {
        if content.is_empty() || content.len() > MAX_DATAGRAM_BYTES {
            return Err(FlowProtocolError::InvalidPayload);
        }
        Ok(Self { endpoint, content })
    }

    #[must_use]
    pub const fn endpoint(&self) -> &FlowEndpoint {
        &self.endpoint
    }

    #[must_use]
    pub fn content(&self) -> &[u8] {
        &self.content
    }

    pub fn encode(&self) -> Result<Vec<u8>, FlowProtocolError> {
        let mut output = Vec::with_capacity(self.content.len() + 20);
        self.endpoint.encode_into(&mut output)?;
        output.extend_from_slice(&self.content);
        Ok(output)
    }

    pub fn decode(input: &[u8]) -> Result<Self, FlowProtocolError> {
        let (endpoint, consumed) = FlowEndpoint::decode(input)?;
        Self::new(endpoint, input[consumed..].to_vec())
    }
}
