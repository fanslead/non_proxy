use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::{
    SnapshotArtifact, SnapshotRecord, SnapshotStatus, StorageError, migration::to_sqlite_u64,
};

pub(crate) fn read_snapshot(
    connection: &Connection,
    snapshot_version: u64,
) -> Result<Option<SnapshotRecord>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT snapshot_version, source_snapshot_version, schema_version,
                created_at_unix_ms, content_hash, policy_count, payload, status,
                activated_at_unix_ms, failure_code
         FROM policy_snapshot WHERE snapshot_version = ?1",
    )?;
    let raw = statement
        .query_row([to_sqlite_u64(snapshot_version)?], read_snapshot_row)
        .optional()?;
    raw.map(decode_snapshot).transpose()
}

pub(crate) fn read_snapshot_by_status(
    connection: &Connection,
    status: SnapshotStatus,
) -> Result<Option<SnapshotRecord>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT snapshot_version, source_snapshot_version, schema_version,
                created_at_unix_ms, content_hash, policy_count, payload, status,
                activated_at_unix_ms, failure_code
         FROM policy_snapshot WHERE status = ?1",
    )?;
    let raw = statement
        .query_row([status.as_str()], read_snapshot_row)
        .optional()?;
    raw.map(decode_snapshot).transpose()
}

pub(crate) fn read_previous_effective_version(
    connection: &Connection,
    active_snapshot_version: u64,
) -> Result<Option<u64>, StorageError> {
    let value: Option<i64> = connection.query_row(
        "SELECT MAX(snapshot_version)
         FROM policy_snapshot
         WHERE status = 'superseded' AND snapshot_version < ?1",
        [to_sqlite_u64(active_snapshot_version)?],
        |row| row.get(0),
    )?;
    value
        .map(|value| decode_u64(value, "policy_snapshot.snapshot_version"))
        .transpose()
}

pub(crate) fn ensure_active_snapshot_version(
    transaction: &Transaction<'_>,
    expected_active_snapshot_version: u64,
) -> Result<(), StorageError> {
    let active: Option<i64> = transaction
        .query_row(
            "SELECT snapshot_version
             FROM policy_snapshot
             WHERE status = 'active'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let active = active
        .map(|value| decode_u64(value, "policy_snapshot.snapshot_version"))
        .transpose()?;
    let matches = active.is_some_and(|value| value == expected_active_snapshot_version);
    if !matches {
        return Err(StorageError::ActiveSnapshotVersionConflict);
    }
    Ok(())
}

type RawSnapshot = (
    i64,
    Option<i64>,
    i64,
    i64,
    Vec<u8>,
    i64,
    Vec<u8>,
    String,
    Option<i64>,
    Option<String>,
);

fn read_snapshot_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawSnapshot> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn decode_snapshot(raw: RawSnapshot) -> Result<SnapshotRecord, StorageError> {
    let content_hash: [u8; 32] = raw.4.try_into().map_err(|_| StorageError::CorruptData {
        field: "policy_snapshot.content_hash",
    })?;
    let artifact = SnapshotArtifact::new(
        decode_u64(raw.0, "policy_snapshot.snapshot_version")?,
        u32::try_from(raw.2).map_err(|_| StorageError::CorruptData {
            field: "policy_snapshot.schema_version",
        })?,
        decode_u64(raw.3, "policy_snapshot.created_at_unix_ms")?,
        content_hash,
        usize::try_from(raw.5).map_err(|_| StorageError::CorruptData {
            field: "policy_snapshot.policy_count",
        })?,
        raw.6,
    )?;
    Ok(SnapshotRecord::new(
        artifact,
        raw.1
            .map(|value| decode_u64(value, "policy_snapshot.source_version"))
            .transpose()?,
        SnapshotStatus::parse(&raw.7)?,
        raw.8
            .map(|value| decode_u64(value, "policy_snapshot.activated_at"))
            .transpose()?,
        raw.9,
    ))
}

fn decode_u64(value: i64, field: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptData { field })
}
