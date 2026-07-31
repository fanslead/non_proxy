use std::net::IpAddr;

use nonproxy_model::OutboundId;

use crate::StorageError;

const PROBE_ID_LENGTH: usize = 43;
const KEY_ID_LENGTH: usize = 22;
const MAXIMUM_RECEIPT_AGE_MS: u64 = 120_000;
const MAXIMUM_FUTURE_SKEW_MS: u64 = 300_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExitProbeRoute {
    Direct,
    Proxy(OutboundId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitProbeInput {
    pub(crate) probe_id: String,
    pub(crate) route: ExitProbeRoute,
    pub(crate) observed_ip: IpAddr,
    pub(crate) observed_at_unix_ms: u64,
    pub(crate) key_id: String,
    pub(crate) verified_at_unix_ms: u64,
}

impl ExitProbeInput {
    pub fn new(
        probe_id: impl Into<String>,
        route: ExitProbeRoute,
        observed_ip: IpAddr,
        observed_at_unix_ms: u64,
        key_id: impl Into<String>,
        verified_at_unix_ms: u64,
    ) -> Result<Self, StorageError> {
        let probe_id = probe_id.into();
        let key_id = key_id.into();
        validate_token(&probe_id, PROBE_ID_LENGTH)?;
        validate_token(&key_id, KEY_ID_LENGTH)?;
        validate_public_ip(observed_ip)?;
        validate_timestamp(observed_at_unix_ms, verified_at_unix_ms)?;
        Ok(Self {
            probe_id,
            route,
            observed_ip,
            observed_at_unix_ms,
            key_id,
            verified_at_unix_ms,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitProbeRecord {
    sequence: i64,
    probe_id: String,
    route: ExitProbeRoute,
    observed_ip: IpAddr,
    observed_at_unix_ms: u64,
    key_id: String,
    verified_at_unix_ms: u64,
}

impl ExitProbeRecord {
    pub(crate) fn new(sequence: i64, input: ExitProbeInput) -> Result<Self, StorageError> {
        if sequence <= 0 {
            return Err(StorageError::CorruptData {
                field: "exit_probe_receipt.sequence",
            });
        }
        Ok(Self {
            sequence,
            probe_id: input.probe_id,
            route: input.route,
            observed_ip: input.observed_ip,
            observed_at_unix_ms: input.observed_at_unix_ms,
            key_id: input.key_id,
            verified_at_unix_ms: input.verified_at_unix_ms,
        })
    }

    #[must_use]
    pub const fn sequence(&self) -> i64 {
        self.sequence
    }

    #[must_use]
    pub fn probe_id(&self) -> &str {
        &self.probe_id
    }

    #[must_use]
    pub const fn route(&self) -> &ExitProbeRoute {
        &self.route
    }

    #[must_use]
    pub const fn observed_ip(&self) -> IpAddr {
        self.observed_ip
    }

    #[must_use]
    pub const fn observed_at_unix_ms(&self) -> u64 {
        self.observed_at_unix_ms
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    #[must_use]
    pub const fn verified_at_unix_ms(&self) -> u64 {
        self.verified_at_unix_ms
    }
}

fn validate_token(value: &str, expected_length: usize) -> Result<(), StorageError> {
    if value.len() != expected_length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(StorageError::ExitProbeInvalid);
    }
    Ok(())
}

fn validate_timestamp(observed_at: u64, verified_at: u64) -> Result<(), StorageError> {
    let oldest = verified_at.saturating_sub(MAXIMUM_RECEIPT_AGE_MS);
    let newest = verified_at
        .checked_add(MAXIMUM_FUTURE_SKEW_MS)
        .ok_or(StorageError::ExitProbeInvalid)?;
    if observed_at == 0 || verified_at == 0 || observed_at < oldest || observed_at > newest {
        return Err(StorageError::ExitProbeInvalid);
    }
    Ok(())
}

fn validate_public_ip(value: IpAddr) -> Result<(), StorageError> {
    let is_public = match value {
        IpAddr::V4(address) => !matches!(
            address.octets(),
            [0, ..]
                | [10, ..]
                | [100, 64..=127, ..]
                | [127, ..]
                | [169, 254, ..]
                | [172, 16..=31, ..]
                | [192, 0, 0, ..]
                | [192, 0, 2, ..]
                | [192, 88, 99, ..]
                | [192, 168, ..]
                | [198, 18..=19, ..]
                | [198, 51, 100, ..]
                | [203, 0, 113, ..]
                | [224..=255, ..]
        ),
        IpAddr::V6(address) => {
            let segments = address.segments();
            segments[0] & 0xe000 == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    };
    if is_public {
        Ok(())
    } else {
        Err(StorageError::ExitProbeInvalid)
    }
}
