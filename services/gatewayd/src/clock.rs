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

pub fn unix_ms_from_timestamp(value: &prost_types::Timestamp) -> Result<u64, GatewayError> {
    if value.seconds < 0 || !(0..1_000_000_000).contains(&value.nanos) {
        return Err(GatewayError::InvalidRequest("时间戳超出有效范围"));
    }
    let seconds =
        u64::try_from(value.seconds).map_err(|_| GatewayError::InvalidRequest("时间戳无效"))?;
    let milliseconds = seconds
        .checked_mul(1_000)
        .and_then(|base| base.checked_add(u64::from(value.nanos.unsigned_abs()) / 1_000_000))
        .ok_or(GatewayError::ClockOverflow)?;
    Ok(milliseconds)
}

pub fn micros_from_duration(value: &prost_types::Duration) -> Result<u64, GatewayError> {
    if value.seconds < 0 || !(0..1_000_000_000).contains(&value.nanos) {
        return Err(GatewayError::InvalidRequest("时长超出有效范围"));
    }
    let seconds =
        u64::try_from(value.seconds).map_err(|_| GatewayError::InvalidRequest("时长无效"))?;
    seconds
        .checked_mul(1_000_000)
        .and_then(|base| base.checked_add(u64::from(value.nanos.unsigned_abs()) / 1_000))
        .ok_or(GatewayError::ClockOverflow)
}

#[cfg(test)]
mod tests {
    use super::{micros_from_duration, unix_ms_from_timestamp};

    #[test]
    fn rejects_non_canonical_protobuf_time_values() {
        assert!(
            unix_ms_from_timestamp(&prost_types::Timestamp {
                seconds: -1,
                nanos: 0,
            })
            .is_err()
        );
        assert!(
            micros_from_duration(&prost_types::Duration {
                seconds: 0,
                nanos: 1_000_000_000,
            })
            .is_err()
        );
    }
}
