use nonproxy_model::{OutboundGroupId, OutboundId};
use rusqlite::{Connection, params};

use crate::{StorageError, migration::to_sqlite_u64};

const EVENT_TYPE: &str = "outbound_group_selection_changed";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundGroupSelectionReason {
    InitialStableMember,
    StableHealthChanged,
    GroupRevisionChanged,
}

impl OutboundGroupSelectionReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InitialStableMember => "NP_OUTBOUND_GROUP_INITIAL_STABLE_MEMBER",
            Self::StableHealthChanged => "NP_OUTBOUND_GROUP_STABLE_HEALTH_CHANGED",
            Self::GroupRevisionChanged => "NP_OUTBOUND_GROUP_REVISION_CHANGED",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundGroupSelectionAudit {
    pub group_id: OutboundGroupId,
    pub group_revision: u64,
    pub previous_outbound_id: Option<OutboundId>,
    pub selected_outbound_id: OutboundId,
    pub snapshot_version: u64,
    pub reason: OutboundGroupSelectionReason,
    pub occurred_at_unix_ms: u64,
    pub event_nonce: [u8; 16],
}

pub struct RuntimeAuditRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> RuntimeAuditRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn record_outbound_group_selection(
        &mut self,
        audit: &OutboundGroupSelectionAudit,
    ) -> Result<(), StorageError> {
        let reason_shape_valid = matches!(
            (audit.previous_outbound_id.as_ref(), audit.reason),
            (None, OutboundGroupSelectionReason::InitialStableMember)
                | (
                    Some(_),
                    OutboundGroupSelectionReason::StableHealthChanged
                        | OutboundGroupSelectionReason::GroupRevisionChanged
                )
        );
        if audit.group_revision == 0
            || audit.snapshot_version == 0
            || audit.event_nonce.iter().all(|byte| *byte == 0)
            || !reason_shape_valid
            || audit.previous_outbound_id.as_ref() == Some(&audit.selected_outbound_id)
        {
            return Err(StorageError::OutboundGroupInvalid);
        }
        let previous = audit
            .previous_outbound_id
            .as_ref()
            .map_or("", OutboundId::as_str);
        self.connection.execute(
            "INSERT INTO audit_event(
                 event_id, event_type, occurred_at_unix_ms, snapshot_version,
                 reason_code, details
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format!("{EVENT_TYPE}:{}", encode_nonce(audit.event_nonce)),
                EVENT_TYPE,
                to_sqlite_u64(audit.occurred_at_unix_ms)?,
                to_sqlite_u64(audit.snapshot_version)?,
                audit.reason.as_str(),
                format!(
                    "group_id={};group_revision={};old_outbound_id={previous};new_outbound_id={}",
                    audit.group_id.as_str(),
                    audit.group_revision,
                    audit.selected_outbound_id.as_str()
                ),
            ],
        )?;
        Ok(())
    }

    #[doc(hidden)]
    pub fn outbound_group_selection_count(&self) -> Result<u64, StorageError> {
        let count = self.connection.query_row(
            "SELECT COUNT(*) FROM audit_event WHERE event_type = ?1",
            [EVENT_TYPE],
            |row| row.get::<_, i64>(0),
        )?;
        u64::try_from(count).map_err(|_| StorageError::CorruptData {
            field: "audit_event.count",
        })
    }
}

fn encode_nonce(nonce: [u8; 16]) -> String {
    nonce.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use nonproxy_model::{OutboundGroupId, OutboundId};
    use rusqlite::Connection;

    use super::{
        OutboundGroupSelectionAudit, OutboundGroupSelectionReason, RuntimeAuditRepository,
    };

    #[test]
    fn selection_audit_contains_members_but_never_a_user_target() {
        let mut connection = Connection::open_in_memory()
            .unwrap_or_else(|error| panic!("审计测试数据库打开失败: {error}"));
        connection
            .execute_batch(
                "CREATE TABLE audit_event (
                    event_id TEXT PRIMARY KEY,
                    event_type TEXT NOT NULL,
                    occurred_at_unix_ms INTEGER NOT NULL,
                    snapshot_version INTEGER,
                    policy_id TEXT,
                    reason_code TEXT,
                    details TEXT
                ) STRICT;",
            )
            .unwrap_or_else(|error| panic!("审计测试表创建失败: {error}"));
        let audit = OutboundGroupSelectionAudit {
            group_id: group_id("daily"),
            group_revision: 3,
            previous_outbound_id: Some(outbound_id("primary")),
            selected_outbound_id: outbound_id("backup"),
            snapshot_version: 8,
            reason: OutboundGroupSelectionReason::StableHealthChanged,
            occurred_at_unix_ms: 1_000,
            event_nonce: [7; 16],
        };

        RuntimeAuditRepository::new(&mut connection)
            .record_outbound_group_selection(&audit)
            .unwrap_or_else(|error| panic!("选择审计写入失败: {error}"));
        let stored = connection
            .query_row(
                "SELECT event_type, snapshot_version, reason_code, details FROM audit_event",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .unwrap_or_else(|error| panic!("选择审计读取失败: {error}"));

        assert_eq!(stored.0, "outbound_group_selection_changed");
        assert_eq!(stored.1, 8);
        assert_eq!(stored.2, "NP_OUTBOUND_GROUP_STABLE_HEALTH_CHANGED");
        assert!(stored.3.contains("group_id=daily"));
        assert!(stored.3.contains("old_outbound_id=primary"));
        assert!(stored.3.contains("new_outbound_id=backup"));
        assert!(!stored.3.contains("example.com"));
        assert!(!stored.3.contains("destination"));
    }

    fn group_id(value: &str) -> OutboundGroupId {
        OutboundGroupId::new(value).unwrap_or_else(|error| panic!("测试组 ID 无效: {error}"))
    }

    fn outbound_id(value: &str) -> OutboundId {
        OutboundId::new(value).unwrap_or_else(|error| panic!("测试出口 ID 无效: {error}"))
    }
}
