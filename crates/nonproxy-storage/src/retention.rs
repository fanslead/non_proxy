use rusqlite::{Connection, TransactionBehavior, params};

use crate::{StorageError, migration::to_sqlite_u64};

pub const DEFAULT_DETAIL_RETENTION_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_CLEANUP_BATCH: usize = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetentionResult {
    decision_rows_deleted: usize,
    dns_rows_deleted: usize,
}

impl RetentionResult {
    #[must_use]
    pub const fn decision_rows_deleted(self) -> usize {
        self.decision_rows_deleted
    }

    #[must_use]
    pub const fn dns_rows_deleted(self) -> usize {
        self.dns_rows_deleted
    }
}

pub struct RetentionRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> RetentionRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn purge_expired_detail(
        &mut self,
        now_unix_ms: u64,
        retention_ms: u64,
        batch_limit: usize,
    ) -> Result<RetentionResult, StorageError> {
        if retention_ms == 0 || batch_limit == 0 || batch_limit > MAX_CLEANUP_BATCH {
            return Err(StorageError::RetentionInvalid);
        }
        let cutoff = now_unix_ms.saturating_sub(retention_ms);
        let limit = i64::try_from(batch_limit).map_err(|_| StorageError::RetentionInvalid)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let decision_rows_deleted = transaction.execute(
            "DELETE FROM connection_decision
             WHERE event_id IN (
                SELECT event_id FROM connection_decision
                WHERE occurred_at_unix_ms < ?1
                ORDER BY occurred_at_unix_ms
                LIMIT ?2
             )",
            params![to_sqlite_u64(cutoff)?, limit],
        )?;
        let dns_rows_deleted = transaction.execute(
            "DELETE FROM dns_observation
             WHERE event_id IN (
                SELECT event_id FROM dns_observation
                WHERE occurred_at_unix_ms < ?1
                ORDER BY occurred_at_unix_ms
                LIMIT ?2
             )",
            params![to_sqlite_u64(cutoff)?, limit],
        )?;
        transaction.commit()?;
        Ok(RetentionResult {
            decision_rows_deleted,
            dns_rows_deleted,
        })
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    use super::*;
    use crate::migration::migrate;

    #[test]
    fn cleanup_is_bounded_and_keeps_recent_details() {
        let mut connection = match Connection::open_in_memory() {
            Ok(value) => value,
            Err(error) => panic!("保留测试数据库打开失败: {error}"),
        };
        if let Err(error) = connection.pragma_update(None, "foreign_keys", "ON") {
            panic!("保留测试外键启用失败: {error}");
        }
        if let Err(error) = migrate(&mut connection, None, 1_000) {
            panic!("保留测试迁移失败: {error}");
        }
        for (id, occurred_at) in [("old-1", 100_i64), ("old-2", 200), ("new", 9_900)] {
            if let Err(error) = connection.execute(
                "INSERT INTO connection_decision(
                    event_id, occurred_at_unix_ms, snapshot_version,
                    app_stable_id, destination_redacted, transport,
                    destination_port, decision_action, reason_code
                 ) VALUES (?1, ?2, 1, 'app', 'redacted', 1, 443, 1, 'NP_TEST')",
                params![id, occurred_at],
            ) {
                panic!("保留测试决策写入失败: {error}");
            }
        }

        let first =
            RetentionRepository::new(&mut connection).purge_expired_detail(10_000, 1_000, 1);
        let Ok(first) = first else {
            panic!("首批保留清理失败: {first:?}");
        };
        assert_eq!(first.decision_rows_deleted(), 1);
        let remaining: i64 =
            match connection.query_row("SELECT COUNT(*) FROM connection_decision", [], |row| {
                row.get(0)
            }) {
                Ok(value) => value,
                Err(error) => panic!("保留测试计数失败: {error}"),
            };
        assert_eq!(remaining, 2);

        let second =
            RetentionRepository::new(&mut connection).purge_expired_detail(10_000, 1_000, 10);
        let Ok(second) = second else {
            panic!("第二批保留清理失败: {second:?}");
        };
        assert_eq!(second.decision_rows_deleted(), 1);
        let recent: String =
            match connection.query_row("SELECT event_id FROM connection_decision", [], |row| {
                row.get(0)
            }) {
                Ok(value) => value,
                Err(error) => panic!("保留测试近期记录读取失败: {error}"),
            };
        assert_eq!(recent, "new");
    }
}
