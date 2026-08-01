use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::{
    StorageError, SubscriptionDeleteCommit, SubscriptionRepository,
    credential_cleanup_repository::enqueue_credential_cleanup, migration::to_sqlite_u64,
    subscription_repository::classify_source_conflict,
};

impl SubscriptionRepository<'_> {
    pub fn delete(
        &mut self,
        source_id: &str,
        expected_revision: u64,
        deleted_at_unix_ms: u64,
    ) -> Result<SubscriptionDeleteCommit, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source = transaction
            .query_row(
                "SELECT revision, endpoint_credential_reference
                 FROM subscription_source WHERE id = ?1",
                [source_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((revision, url_reference)) = source else {
            return Err(StorageError::SubscriptionRevisionConflict);
        };
        if u64::try_from(revision) != Ok(expected_revision) {
            return Err(classify_source_conflict(
                &transaction,
                source_id,
                expected_revision,
            )?);
        }

        let outbounds = owned_outbounds(&transaction, source_id)?;
        reject_referenced_outbounds(&transaction, &outbounds)?;
        let mut credentials = vec![url_reference];
        credentials.extend(
            outbounds
                .iter()
                .filter_map(|(_, credential)| credential.clone()),
        );
        enqueue_credential_cleanup(
            &transaction,
            credentials.iter().cloned(),
            deleted_at_unix_ms,
        )?;

        let deleted = transaction.execute(
            "DELETE FROM subscription_source WHERE id = ?1 AND revision = ?2",
            params![source_id, to_sqlite_u64(expected_revision)?],
        )?;
        if deleted != 1 {
            return Err(StorageError::SubscriptionRevisionConflict);
        }
        for (outbound_id, _) in &outbounds {
            let deleted =
                transaction.execute("DELETE FROM outbound WHERE id = ?1", [outbound_id])?;
            if deleted != 1 {
                return Err(StorageError::SubscriptionOwnershipConflict);
            }
        }
        transaction.execute(
            "INSERT INTO audit_event(event_id, event_type, occurred_at_unix_ms, details)
             VALUES (?1, 'subscription_deleted', ?2, ?3)",
            params![
                format!("subscription_deleted:{source_id}:r{expected_revision}"),
                to_sqlite_u64(deleted_at_unix_ms)?,
                format!(
                    "subscription_id={source_id};revision={expected_revision};outbounds={}",
                    outbounds.len()
                ),
            ],
        )?;
        transaction.commit()?;
        Ok(SubscriptionDeleteCommit::new(credentials, outbounds.len()))
    }
}

fn owned_outbounds(
    transaction: &rusqlite::Transaction<'_>,
    source_id: &str,
) -> Result<Vec<(String, Option<String>)>, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT so.outbound_id, o.credential_reference
         FROM subscription_outbound so
         JOIN outbound o ON o.id = so.outbound_id
         WHERE so.subscription_id = ?1 ORDER BY so.outbound_id",
    )?;
    let rows = statement.query_map([source_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn reject_referenced_outbounds(
    transaction: &rusqlite::Transaction<'_>,
    outbounds: &[(String, Option<String>)],
) -> Result<(), StorageError> {
    for (outbound_id, _) in outbounds {
        let is_default = transaction.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM routing_settings
                 WHERE singleton_id = 1 AND default_action = 'proxy'
                   AND default_outbound_id = ?1
             )",
            [outbound_id],
            |row| row.get::<_, bool>(0),
        )?;
        if is_default {
            return Err(StorageError::SubscriptionDefaultOutboundRemoved);
        }
        let in_use = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM policy WHERE outbound_id = ?1)
                 OR EXISTS(SELECT 1 FROM outbound_group_member WHERE outbound_id = ?1)",
            [outbound_id],
            |row| row.get::<_, bool>(0),
        )?;
        if in_use {
            return Err(StorageError::SubscriptionOutboundInUse);
        }
    }
    Ok(())
}
