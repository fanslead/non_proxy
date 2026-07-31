use nonproxy_model::{FailureMode, Platform, RouteAction, Transport};
use rusqlite::{Connection, OptionalExtension};

use crate::{ConnectionDecisionInput, ConnectionDecisionRecord, EvidenceLevel, StorageError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedDecision {
    pub(crate) occurred_at_unix_ms: u64,
    pub(crate) snapshot_version: u64,
    pub(crate) app_stable_id: String,
    pub(crate) app_display_name: Option<String>,
    pub(crate) app_platform: Platform,
    pub(crate) destination_redacted: String,
    pub(crate) transport: Transport,
    pub(crate) destination_port: u16,
    pub(crate) matched_policy_id: Option<String>,
    pub(crate) matched_rule_id: Option<String>,
    pub(crate) action: RouteAction,
    pub(crate) failure_mode: FailureMode,
    pub(crate) reason_code: String,
    pub(crate) evidence_level: EvidenceLevel,
    pub(crate) interface_name: Option<String>,
    pub(crate) outbound_id: Option<String>,
    pub(crate) exit_probe_id: Option<String>,
    pub(crate) fail_open_direct: bool,
    pub(crate) decision_latency_micros: Option<u64>,
    pub(crate) error_code: Option<String>,
    pub(crate) provider_id: String,
    pub(crate) provider_generation: u64,
    pub(crate) flow_id: String,
}

pub(crate) fn persisted(
    input: &ConnectionDecisionInput,
) -> Result<PersistedDecision, StorageError> {
    let destination_redacted = input
        .destination
        .domain()
        .map(|value| value.as_ascii().to_owned())
        .or_else(|| input.destination.ip().map(|value| value.to_string()))
        .ok_or(StorageError::ConnectionDecisionInvalid)?;
    Ok(PersistedDecision {
        occurred_at_unix_ms: input.occurred_at_unix_ms,
        snapshot_version: input.decision.snapshot_version(),
        app_stable_id: input.app.stable_id().to_owned(),
        app_display_name: input.app.display_name().map(str::to_owned),
        app_platform: input.app.platform(),
        destination_redacted,
        transport: input.destination.transport(),
        destination_port: input.destination.port(),
        matched_policy_id: input
            .decision
            .matched_policy_id()
            .map(|value| value.as_str().to_owned()),
        matched_rule_id: input
            .decision
            .matched_rule_id()
            .map(|value| value.as_str().to_owned()),
        action: input.decision.result().action(),
        failure_mode: input.decision.result().failure_mode(),
        reason_code: input.decision.reason_code().to_owned(),
        evidence_level: input.evidence.level(),
        interface_name: input.evidence.interface_name().map(str::to_owned),
        outbound_id: input
            .evidence
            .outbound_id()
            .map(|value| value.as_str().to_owned()),
        exit_probe_id: input.evidence.exit_probe_id().map(str::to_owned),
        fail_open_direct: input.evidence.fail_open_direct(),
        decision_latency_micros: input.decision_latency_micros,
        error_code: input.error_code.clone(),
        provider_id: input.provider_id.clone(),
        provider_generation: input.provider_generation,
        flow_id: input.flow_id.clone(),
    })
}

pub(crate) fn read_persisted(
    connection: &Connection,
    event_id: &str,
) -> Result<Option<PersistedDecision>, StorageError> {
    connection
        .query_row(
            "SELECT occurred_at_unix_ms, snapshot_version, app_stable_id,
                    app_display_name, app_platform, destination_redacted,
                    transport, destination_port, matched_policy_id,
                    matched_rule_id, decision_action, failure_mode, reason_code,
                    evidence_level, interface_name, outbound_id, exit_probe_id,
                    fail_open_direct, decision_latency_us, error_code,
                    provider_id, provider_generation, flow_id
             FROM connection_decision WHERE event_id = ?1",
            [event_id],
            decode_persisted,
        )
        .optional()
        .map_err(StorageError::from)
}

pub(crate) fn decode_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConnectionDecisionRecord> {
    Ok(ConnectionDecisionRecord {
        sequence: row.get(0)?,
        event_id: row.get(1)?,
        persisted: decode_persisted_at(row, 2)?,
    })
}

fn decode_persisted(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedDecision> {
    decode_persisted_at(row, 0)
}

fn decode_persisted_at(
    row: &rusqlite::Row<'_>,
    start: usize,
) -> rusqlite::Result<PersistedDecision> {
    Ok(PersistedDecision {
        occurred_at_unix_ms: decode_u64(row, start, "connection_decision.occurred_at_unix_ms")?,
        snapshot_version: decode_u64(row, start + 1, "connection_decision.snapshot_version")?,
        app_stable_id: row.get(start + 2)?,
        app_display_name: row.get(start + 3)?,
        app_platform: parse_platform(row.get(start + 4)?).map_err(storage_to_sql)?,
        destination_redacted: row.get(start + 5)?,
        transport: parse_transport(row.get(start + 6)?).map_err(storage_to_sql)?,
        destination_port: decode_u16(row, start + 7, "connection_decision.destination_port")?,
        matched_policy_id: row.get(start + 8)?,
        matched_rule_id: row.get(start + 9)?,
        action: parse_action(row.get(start + 10)?).map_err(storage_to_sql)?,
        failure_mode: parse_failure_mode(row.get(start + 11)?).map_err(storage_to_sql)?,
        reason_code: row.get(start + 12)?,
        evidence_level: EvidenceLevel::parse(row.get(start + 13)?).map_err(storage_to_sql)?,
        interface_name: row.get(start + 14)?,
        outbound_id: row.get(start + 15)?,
        exit_probe_id: row.get(start + 16)?,
        fail_open_direct: decode_bool(row, start + 17, "connection_decision.fail_open_direct")?,
        decision_latency_micros: decode_optional_u64(
            row,
            start + 18,
            "connection_decision.decision_latency_us",
        )?,
        error_code: row.get(start + 19)?,
        provider_id: row.get(start + 20)?,
        provider_generation: decode_u64(
            row,
            start + 21,
            "connection_decision.provider_generation",
        )?,
        flow_id: row.get(start + 22)?,
    })
}

pub(crate) const fn platform_code(value: Platform) -> i64 {
    match value {
        Platform::MacOs => 1,
        Platform::Windows => 2,
    }
}

pub(crate) const fn transport_code(value: Transport) -> i64 {
    match value {
        Transport::Tcp => 1,
        Transport::Udp => 2,
    }
}

pub(crate) const fn action_code(value: RouteAction) -> i64 {
    match value {
        RouteAction::Direct => 1,
        RouteAction::Proxy => 2,
        RouteAction::Block => 3,
    }
}

pub(crate) const fn failure_mode_code(value: FailureMode) -> i64 {
    match value {
        FailureMode::Closed => 1,
        FailureMode::Open => 2,
    }
}

fn parse_platform(value: i64) -> Result<Platform, StorageError> {
    match value {
        1 => Ok(Platform::MacOs),
        2 => Ok(Platform::Windows),
        _ => Err(StorageError::CorruptData {
            field: "connection_decision.app_platform",
        }),
    }
}

fn parse_transport(value: i64) -> Result<Transport, StorageError> {
    match value {
        1 => Ok(Transport::Tcp),
        2 => Ok(Transport::Udp),
        _ => Err(StorageError::CorruptData {
            field: "connection_decision.transport",
        }),
    }
}

fn parse_action(value: i64) -> Result<RouteAction, StorageError> {
    match value {
        1 => Ok(RouteAction::Direct),
        2 => Ok(RouteAction::Proxy),
        3 => Ok(RouteAction::Block),
        _ => Err(StorageError::CorruptData {
            field: "connection_decision.decision_action",
        }),
    }
}

fn parse_failure_mode(value: i64) -> Result<FailureMode, StorageError> {
    match value {
        1 => Ok(FailureMode::Closed),
        2 => Ok(FailureMode::Open),
        _ => Err(StorageError::CorruptData {
            field: "connection_decision.failure_mode",
        }),
    }
}

fn decode_u64(row: &rusqlite::Row<'_>, index: usize, field: &'static str) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|_| corrupt_sql(field))
}

fn decode_optional_u64(
    row: &rusqlite::Row<'_>,
    index: usize,
    field: &'static str,
) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(index)?
        .map(|value| u64::try_from(value).map_err(|_| corrupt_sql(field)))
        .transpose()
}

fn decode_u16(row: &rusqlite::Row<'_>, index: usize, field: &'static str) -> rusqlite::Result<u16> {
    u16::try_from(row.get::<_, i64>(index)?).map_err(|_| corrupt_sql(field))
}

fn decode_bool(
    row: &rusqlite::Row<'_>,
    index: usize,
    field: &'static str,
) -> rusqlite::Result<bool> {
    match row.get::<_, i64>(index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(corrupt_sql(field)),
    }
}

fn corrupt_sql(field: &'static str) -> rusqlite::Error {
    storage_to_sql(StorageError::CorruptData { field })
}

fn storage_to_sql(error: StorageError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Integer, Box::new(error))
}
