use std::{net::IpAddr, str::FromStr};

use nonproxy_model::OutboundId;
use rusqlite::{Connection, Row, Transaction, TransactionBehavior, params};

use crate::{
    ExitProbeInput, ExitProbeRecord, ExitProbeRoute, StorageError, migration::to_sqlite_u64,
};

const MAX_PAGE_SIZE: usize = 500;
const MAX_STORED_RECEIPTS: i64 = 2_048;

pub struct ExitProbeRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> ExitProbeRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save(&mut self, input: &ExitProbeInput) -> Result<i64, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let sequence = match read_by_probe_id(&transaction, &input.probe_id)? {
            Some(existing) if existing == *input => read_sequence(&transaction, &input.probe_id)?,
            Some(_) => return Err(StorageError::ExitProbeReplayMismatch),
            None => insert(&transaction, input)?,
        };
        transaction.execute(
            "DELETE FROM exit_probe_receipt
             WHERE sequence IN (
                 SELECT sequence FROM exit_probe_receipt
                 ORDER BY sequence DESC
                 LIMIT -1 OFFSET ?1
             )",
            [MAX_STORED_RECEIPTS],
        )?;
        transaction.commit()?;
        Ok(sequence)
    }

    pub fn list_recent(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<ExitProbeRecord>, u64), StorageError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(StorageError::ExitProbeInvalid);
        }
        let total =
            self.connection
                .query_row("SELECT COUNT(*) FROM exit_probe_receipt", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let mut statement = self.connection.prepare(
            "SELECT sequence, probe_id, route_kind, outbound_id, observed_ip,
                    observed_at_unix_ms, key_id, verified_at_unix_ms
             FROM exit_probe_receipt
             ORDER BY sequence DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = statement.query_map(
            params![usize_to_i64(limit)?, usize_to_i64(offset)?],
            decode_record,
        )?;
        Ok((
            rows.collect::<Result<Vec<_>, _>>()?,
            u64::try_from(total).map_err(|_| StorageError::CorruptData {
                field: "exit_probe_receipt.count",
            })?,
        ))
    }
}

fn insert(transaction: &Transaction<'_>, input: &ExitProbeInput) -> Result<i64, StorageError> {
    let (route_kind, outbound_id) = route_columns(&input.route);
    transaction.execute(
        "INSERT INTO exit_probe_receipt(
             probe_id, route_kind, outbound_id, observed_ip, ip_family,
             observed_at_unix_ms, key_id, verified_at_unix_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            input.probe_id,
            route_kind,
            outbound_id,
            input.observed_ip.to_string(),
            ip_family(input.observed_ip),
            to_sqlite_u64(input.observed_at_unix_ms)?,
            input.key_id,
            to_sqlite_u64(input.verified_at_unix_ms)?,
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

fn read_by_probe_id(
    transaction: &Transaction<'_>,
    probe_id: &str,
) -> Result<Option<ExitProbeInput>, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT route_kind, outbound_id, observed_ip, observed_at_unix_ms,
                key_id, verified_at_unix_ms
         FROM exit_probe_receipt WHERE probe_id = ?1",
    )?;
    let mut rows = statement.query([probe_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(decode_input(row, probe_id.to_owned())?))
}

fn read_sequence(transaction: &Transaction<'_>, probe_id: &str) -> Result<i64, StorageError> {
    transaction
        .query_row(
            "SELECT sequence FROM exit_probe_receipt WHERE probe_id = ?1",
            [probe_id],
            |row| row.get(0),
        )
        .map_err(StorageError::from)
}

fn decode_record(row: &Row<'_>) -> rusqlite::Result<ExitProbeRecord> {
    let sequence = row.get::<_, i64>(0)?;
    let probe_id = row.get::<_, String>(1)?;
    let input = decode_input_columns(row, 2, probe_id).map_err(to_sql_error)?;
    ExitProbeRecord::new(sequence, input).map_err(to_sql_error)
}

fn decode_input(row: &Row<'_>, probe_id: String) -> Result<ExitProbeInput, StorageError> {
    decode_input_columns(row, 0, probe_id)
}

fn decode_input_columns(
    row: &Row<'_>,
    start: usize,
    probe_id: String,
) -> Result<ExitProbeInput, StorageError> {
    let route_kind = row.get::<_, i64>(start)?;
    let outbound_id = row.get::<_, Option<String>>(start + 1)?;
    let route = decode_route(route_kind, outbound_id)?;
    let observed_ip = IpAddr::from_str(&row.get::<_, String>(start + 2)?)
        .map_err(|_| corrupt("exit_probe_receipt.observed_ip"))?;
    let observed_at = decode_u64(row, start + 3, "exit_probe_receipt.observed_at_unix_ms")?;
    let key_id = row.get::<_, String>(start + 4)?;
    let verified_at = decode_u64(row, start + 5, "exit_probe_receipt.verified_at_unix_ms")?;
    ExitProbeInput::new(
        probe_id,
        route,
        observed_ip,
        observed_at,
        key_id,
        verified_at,
    )
}

fn decode_route(kind: i64, outbound_id: Option<String>) -> Result<ExitProbeRoute, StorageError> {
    match (kind, outbound_id) {
        (1, None) => Ok(ExitProbeRoute::Direct),
        (2, Some(value)) => OutboundId::new(value)
            .map(ExitProbeRoute::Proxy)
            .map_err(StorageError::from),
        _ => Err(corrupt("exit_probe_receipt.route")),
    }
}

fn route_columns(route: &ExitProbeRoute) -> (i64, Option<&str>) {
    match route {
        ExitProbeRoute::Direct => (1, None),
        ExitProbeRoute::Proxy(outbound_id) => (2, Some(outbound_id.as_str())),
    }
}

const fn ip_family(value: IpAddr) -> i64 {
    match value {
        IpAddr::V4(_) => 1,
        IpAddr::V6(_) => 2,
    }
}

fn decode_u64(row: &Row<'_>, index: usize, field: &'static str) -> Result<u64, StorageError> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|_| corrupt(field))
}

fn usize_to_i64(value: usize) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ExitProbeInvalid)
}

const fn corrupt(field: &'static str) -> StorageError {
    StorageError::CorruptData { field }
}

fn to_sql_error(error: StorageError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}
