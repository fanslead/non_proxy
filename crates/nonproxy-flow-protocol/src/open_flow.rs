use std::fmt;

use nonproxy_model::{OutboundGroupId, OutboundId};
use zeroize::{Zeroize, Zeroizing};

use crate::{FlowEndpoint, FlowProtocolError};

pub const CAPABILITY_TOKEN_BYTES: usize = 32;
const MINIMUM_WINDOW_BYTES: u32 = 16 * 1024;
const MAXIMUM_WINDOW_BYTES: u32 = 16 * 1024 * 1024;

pub struct OpenFlowRequest {
    capability: CapabilityToken,
    proxy_target: FlowProxyTarget,
    endpoint: FlowEndpoint,
    initial_window_bytes: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowProxyTarget {
    Outbound(OutboundId),
    Group {
        id: OutboundGroupId,
        snapshot_version: u64,
    },
}

struct CapabilityToken([u8; CAPABILITY_TOKEN_BYTES]);

impl OpenFlowRequest {
    pub fn new(
        capability: [u8; CAPABILITY_TOKEN_BYTES],
        outbound_id: OutboundId,
        endpoint: FlowEndpoint,
        initial_window_bytes: u32,
    ) -> Result<Self, FlowProtocolError> {
        Self::new_with_target(
            capability,
            FlowProxyTarget::Outbound(outbound_id),
            endpoint,
            initial_window_bytes,
        )
    }

    pub fn new_with_target(
        capability: [u8; CAPABILITY_TOKEN_BYTES],
        proxy_target: FlowProxyTarget,
        endpoint: FlowEndpoint,
        initial_window_bytes: u32,
    ) -> Result<Self, FlowProtocolError> {
        if !(MINIMUM_WINDOW_BYTES..=MAXIMUM_WINDOW_BYTES).contains(&initial_window_bytes) {
            return Err(FlowProtocolError::InvalidPayload);
        }
        if matches!(
            proxy_target,
            FlowProxyTarget::Group {
                snapshot_version: 0,
                ..
            }
        ) {
            return Err(FlowProtocolError::InvalidPayload);
        }
        Ok(Self {
            capability: CapabilityToken(capability),
            proxy_target,
            endpoint,
            initial_window_bytes,
        })
    }

    #[must_use]
    pub fn capability(&self) -> &[u8; CAPABILITY_TOKEN_BYTES] {
        &self.capability.0
    }

    #[must_use]
    pub const fn proxy_target(&self) -> &FlowProxyTarget {
        &self.proxy_target
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
        let target = match &self.proxy_target {
            FlowProxyTarget::Outbound(id) => id.as_str().as_bytes(),
            FlowProxyTarget::Group { id, .. } => id.as_str().as_bytes(),
        };
        let target_length =
            u8::try_from(target.len()).map_err(|_| FlowProtocolError::InvalidPayload)?;
        let mut output = Zeroizing::new(Vec::with_capacity(74 + target.len()));
        output.extend_from_slice(self.capability());
        match &self.proxy_target {
            FlowProxyTarget::Outbound(_) => {
                output.push(target_length);
                output.extend_from_slice(target);
            }
            FlowProxyTarget::Group {
                snapshot_version, ..
            } => {
                output.extend_from_slice(&[0, 2, target_length]);
                output.extend_from_slice(target);
                output.extend_from_slice(&snapshot_version.to_be_bytes());
            }
        }
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
        let (proxy_target, endpoint_start) = if input[CAPABILITY_TOKEN_BYTES] == 0 {
            decode_extended_target(input)?
        } else {
            decode_legacy_target(input)?
        };
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
        Self::new_with_target(capability, proxy_target, endpoint, initial_window_bytes)
    }
}

fn decode_legacy_target(input: &[u8]) -> Result<(FlowProxyTarget, usize), FlowProtocolError> {
    let length = usize::from(input[CAPABILITY_TOKEN_BYTES]);
    let start = CAPABILITY_TOKEN_BYTES + 1;
    let end = start
        .checked_add(length)
        .ok_or(FlowProtocolError::InvalidPayload)?;
    if input.len() < end {
        return Err(FlowProtocolError::InvalidPayload);
    }
    let value =
        std::str::from_utf8(&input[start..end]).map_err(|_| FlowProtocolError::InvalidPayload)?;
    let id = OutboundId::new(value).map_err(FlowProtocolError::InvalidOutbound)?;
    Ok((FlowProxyTarget::Outbound(id), end))
}

fn decode_extended_target(input: &[u8]) -> Result<(FlowProxyTarget, usize), FlowProtocolError> {
    let header_end = CAPABILITY_TOKEN_BYTES + 3;
    if input.len() < header_end || input[CAPABILITY_TOKEN_BYTES + 1] != 2 {
        return Err(FlowProtocolError::InvalidPayload);
    }
    let length = usize::from(input[CAPABILITY_TOKEN_BYTES + 2]);
    let id_end = header_end
        .checked_add(length)
        .ok_or(FlowProtocolError::InvalidPayload)?;
    let version_end = id_end
        .checked_add(8)
        .ok_or(FlowProtocolError::InvalidPayload)?;
    if length == 0 || input.len() < version_end {
        return Err(FlowProtocolError::InvalidPayload);
    }
    let value = std::str::from_utf8(&input[header_end..id_end])
        .map_err(|_| FlowProtocolError::InvalidPayload)?;
    let id = OutboundGroupId::new(value).map_err(FlowProtocolError::InvalidOutboundGroup)?;
    let snapshot_version = u64::from_be_bytes(
        input[id_end..version_end]
            .try_into()
            .map_err(|_| FlowProtocolError::InvalidPayload)?,
    );
    if snapshot_version == 0 {
        return Err(FlowProtocolError::InvalidPayload);
    }
    Ok((
        FlowProxyTarget::Group {
            id,
            snapshot_version,
        },
        version_end,
    ))
}

impl fmt::Debug for OpenFlowRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenFlowRequest")
            .field("capability", &"[REDACTED]")
            .field("proxy_target", &self.proxy_target)
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
