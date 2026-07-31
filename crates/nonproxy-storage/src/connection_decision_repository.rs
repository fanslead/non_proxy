use rusqlite::{Connection, Transaction, TransactionBehavior, params};

use crate::{
    ConnectionDecisionInput, ConnectionDecisionRecord, StorageError,
    connection_decision_codec::{
        action_code, decode_record, failure_mode_code, persisted, platform_code, read_persisted,
        transport_code,
    },
    migration::to_sqlite_u64,
};

const MAX_BATCH_SIZE: usize = 1_000;
const MAX_PAGE_SIZE: usize = 500;

pub struct ConnectionDecisionRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> ConnectionDecisionRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save_batch(
        &mut self,
        decisions: &[ConnectionDecisionInput],
    ) -> Result<Vec<usize>, StorageError> {
        if decisions.is_empty() || decisions.len() > MAX_BATCH_SIZE {
            return Err(StorageError::ConnectionDecisionInvalid);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut inserted_indices = Vec::with_capacity(decisions.len());
        for (index, decision) in decisions.iter().enumerate() {
            if save_or_validate_replay(&transaction, decision)? {
                inserted_indices.push(index);
            }
        }
        transaction.commit()?;
        Ok(inserted_indices)
    }

    pub fn list_recent(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<ConnectionDecisionRecord>, u64), StorageError> {
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(StorageError::ConnectionDecisionInvalid);
        }
        let total =
            self.connection
                .query_row("SELECT COUNT(*) FROM connection_decision", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let mut statement = self.connection.prepare(
            "SELECT rowid, event_id, occurred_at_unix_ms, snapshot_version,
                    app_stable_id, app_display_name, app_platform,
                    destination_redacted, transport, destination_port,
                    matched_policy_id, matched_rule_id, decision_action,
                    failure_mode, reason_code, evidence_level, interface_name,
                    outbound_id, exit_probe_id, fail_open_direct,
                    decision_latency_us, error_code, provider_id,
                    provider_generation, flow_id
             FROM connection_decision
             ORDER BY occurred_at_unix_ms DESC, rowid DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = statement.query_map(
            params![usize_to_i64(limit)?, usize_to_i64(offset)?],
            decode_record,
        )?;
        let records = rows.collect::<Result<Vec<_>, _>>()?;
        Ok((
            records,
            u64::try_from(total).map_err(|_| StorageError::CorruptData {
                field: "connection_decision.count",
            })?,
        ))
    }
}

fn save_or_validate_replay(
    transaction: &Transaction<'_>,
    input: &ConnectionDecisionInput,
) -> Result<bool, StorageError> {
    let persisted = persisted(input)?;
    let event_id = format!(
        "{}:{}:{}",
        input.provider_id, input.provider_generation, input.flow_id
    );
    if let Some(existing) = read_persisted(transaction, &event_id)? {
        return if existing == persisted {
            Ok(false)
        } else {
            Err(StorageError::ConnectionDecisionReplayMismatch)
        };
    }
    transaction.execute(
        "INSERT INTO connection_decision(
             event_id, occurred_at_unix_ms, snapshot_version, app_stable_id,
             destination_redacted, transport, destination_port, matched_policy_id,
             decision_action, reason_code, provider_id, provider_generation,
             flow_id, app_display_name, app_platform, matched_rule_id,
             failure_mode, evidence_level, interface_name, outbound_id,
             exit_probe_id, fail_open_direct, decision_latency_us, error_code
         ) VALUES (
             ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
             ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
             ?24
         )",
        params![
            event_id,
            to_sqlite_u64(persisted.occurred_at_unix_ms)?,
            to_sqlite_u64(persisted.snapshot_version)?,
            persisted.app_stable_id,
            persisted.destination_redacted,
            transport_code(persisted.transport),
            i64::from(persisted.destination_port),
            persisted.matched_policy_id,
            action_code(persisted.action),
            persisted.reason_code,
            persisted.provider_id,
            to_sqlite_u64(persisted.provider_generation)?,
            persisted.flow_id,
            persisted.app_display_name,
            platform_code(persisted.app_platform),
            persisted.matched_rule_id,
            failure_mode_code(persisted.failure_mode),
            persisted.evidence_level.as_i64(),
            persisted.interface_name,
            persisted.outbound_id,
            persisted.exit_probe_id,
            i64::from(persisted.fail_open_direct),
            persisted
                .decision_latency_micros
                .map(to_sqlite_u64)
                .transpose()?,
            persisted.error_code,
        ],
    )?;
    Ok(true)
}

fn usize_to_i64(value: usize) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::ConnectionDecisionInvalid)
}
