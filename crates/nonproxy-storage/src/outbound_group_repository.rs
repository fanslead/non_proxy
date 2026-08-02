use nonproxy_model::{OutboundGroupId, OutboundId};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    OutboundGroup, OutboundGroupStrategy, OutboundKind, StorageError, migration::to_sqlite_u64,
};

pub struct OutboundGroupRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> OutboundGroupRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save(
        &mut self,
        group: &OutboundGroup,
        expected_current_revision: Option<u64>,
        updated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_revision(&transaction, group, expected_current_revision)?;
        validate_members(&transaction, group.members())?;
        transaction.execute(
            "INSERT INTO outbound_group(
                 id, display_name, strategy, revision, updated_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 display_name = excluded.display_name,
                 strategy = excluded.strategy,
                 revision = excluded.revision,
                 updated_at_unix_ms = excluded.updated_at_unix_ms",
            params![
                group.id().as_str(),
                group.display_name(),
                group.strategy().as_str(),
                to_sqlite_u64(group.revision())?,
                to_sqlite_u64(updated_at_unix_ms)?,
            ],
        )?;
        transaction.execute(
            "DELETE FROM outbound_group_member WHERE group_id = ?1",
            [group.id().as_str()],
        )?;
        for (position, member) in group.members().iter().enumerate() {
            transaction.execute(
                "INSERT INTO outbound_group_member(group_id, outbound_id, position)
                 VALUES (?1, ?2, ?3)",
                params![
                    group.id().as_str(),
                    member.as_str(),
                    i64::try_from(position).map_err(|_| StorageError::OutboundGroupInvalid)?,
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO audit_event(event_id, event_type, occurred_at_unix_ms, details)
             VALUES (?1, 'outbound_group_saved', ?2, ?3)",
            params![
                format!(
                    "outbound_group_saved:{}:r{}",
                    group.id().as_str(),
                    group.revision()
                ),
                to_sqlite_u64(updated_at_unix_ms)?,
                format!(
                    "group_id={};revision={};member_count={}",
                    group.id().as_str(),
                    group.revision(),
                    group.members().len()
                ),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get(&self, group_id: &OutboundGroupId) -> Result<Option<OutboundGroup>, StorageError> {
        let raw = self
            .connection
            .query_row(
                "SELECT display_name, strategy, revision
                 FROM outbound_group WHERE id = ?1",
                [group_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|raw| self.decode(group_id.clone(), raw))
            .transpose()
    }

    pub fn list(&self) -> Result<Vec<OutboundGroup>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM outbound_group ORDER BY id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        ids.into_iter()
            .map(|id| {
                let group_id = OutboundGroupId::new(id)?;
                self.get(&group_id)?.ok_or(StorageError::CorruptData {
                    field: "outbound_group.id",
                })
            })
            .collect()
    }

    pub fn delete(
        &mut self,
        group_id: &OutboundGroupId,
        expected_revision: u64,
        deleted_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = current_revision(&transaction, group_id)?;
        if current != Some(expected_revision) {
            return Err(StorageError::OutboundGroupRevisionConflict);
        }
        let deleted = transaction.execute(
            "DELETE FROM outbound_group WHERE id = ?1",
            [group_id.as_str()],
        )?;
        if deleted != 1 {
            return Err(StorageError::OutboundGroupRevisionConflict);
        }
        transaction.execute(
            "INSERT INTO audit_event(event_id, event_type, occurred_at_unix_ms, details)
             VALUES (?1, 'outbound_group_deleted', ?2, ?3)",
            params![
                format!(
                    "outbound_group_deleted:{}:r{}",
                    group_id.as_str(),
                    expected_revision
                ),
                to_sqlite_u64(deleted_at_unix_ms)?,
                format!(
                    "group_id={};revision={expected_revision}",
                    group_id.as_str()
                ),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn decode(
        &self,
        id: OutboundGroupId,
        raw: (String, String, i64),
    ) -> Result<OutboundGroup, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT outbound_id, position
             FROM outbound_group_member
             WHERE group_id = ?1 ORDER BY position",
        )?;
        let raw_members = statement
            .query_map([id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut members = Vec::with_capacity(raw_members.len());
        for (expected_position, (outbound_id, position)) in raw_members.into_iter().enumerate() {
            if usize::try_from(position).ok() != Some(expected_position) {
                return Err(StorageError::CorruptData {
                    field: "outbound_group_member.position",
                });
            }
            members.push(OutboundId::new(outbound_id)?);
        }
        let revision = u64::try_from(raw.2).map_err(|_| StorageError::CorruptData {
            field: "outbound_group.revision",
        })?;
        OutboundGroup::new(
            id,
            raw.0,
            OutboundGroupStrategy::parse(&raw.1)?,
            members,
            revision,
        )
    }
}

fn validate_revision(
    transaction: &Transaction<'_>,
    group: &OutboundGroup,
    expected_current_revision: Option<u64>,
) -> Result<(), StorageError> {
    let current = current_revision(transaction, group.id())?;
    let valid = match (current, expected_current_revision) {
        (None, None) => group.revision() == 1,
        (Some(current), Some(expected)) => {
            current == expected && current.checked_add(1) == Some(group.revision())
        }
        _ => false,
    };
    if !valid {
        return Err(StorageError::OutboundGroupRevisionConflict);
    }
    Ok(())
}

fn current_revision(
    transaction: &Transaction<'_>,
    group_id: &OutboundGroupId,
) -> Result<Option<u64>, StorageError> {
    transaction
        .query_row(
            "SELECT revision FROM outbound_group WHERE id = ?1",
            [group_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "outbound_group.revision",
            })
        })
        .transpose()
}

fn validate_members(
    transaction: &Transaction<'_>,
    members: &[OutboundId],
) -> Result<(), StorageError> {
    for member in members {
        let kind = transaction
            .query_row(
                "SELECT kind FROM outbound WHERE id = ?1",
                [member.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(kind) = kind else {
            return Err(StorageError::OutboundGroupMemberNotFound);
        };
        if OutboundKind::parse(&kind)? == OutboundKind::Adapter {
            return Err(StorageError::OutboundGroupMemberUnsupported);
        }
    }
    Ok(())
}
