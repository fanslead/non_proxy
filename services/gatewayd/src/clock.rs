use std::time::{SystemTime, UNIX_EPOCH};

use crate::GatewayError;

pub fn unix_time_ms() -> Result<u64, GatewayError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| GatewayError::ClockBeforeUnixEpoch)?;
    u64::try_from(duration.as_millis()).map_err(|_| GatewayError::ClockOverflow)
}

pub fn timestamp_from_unix_ms(value: u64) -> Result<prost_types::Timestamp, GatewayError> {
    let seconds = i64::try_from(value / 1_000).map_err(|_| GatewayError::ClockOverflow)?;
    let nanos =
        i32::try_from((value % 1_000) * 1_000_000).map_err(|_| GatewayError::ClockOverflow)?;
    Ok(prost_types::Timestamp { seconds, nanos })
}
