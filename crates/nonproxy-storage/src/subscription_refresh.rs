use std::collections::{HashMap, HashSet};

use nonproxy_model::OutboundId;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    CredentialReference, StorageError, SubscriptionNode, SubscriptionNodeOwnership,
    SubscriptionRefreshCommit, SubscriptionRepository, SubscriptionSource,
    migration::to_sqlite_u64,
    outbound_repository::{save_outbound, validate_default_outbound, validate_revision},
    subscription_codec::decode_ownership,
    subscription_refresh_state::{audit_refresh, update_refresh_state, validate_source_state},
    subscription_repository::{save_source, validate_source_revision},
};

impl SubscriptionRepository<'_> {
    #[allow(clippy::too_many_arguments)]
    pub fn apply_refresh(
        &mut self,
        source_id: &str,
        expected_source_revision: u64,
        expected_generation: u64,
        content_hash: [u8; 32],
        nodes: &[SubscriptionNode],
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<SubscriptionRefreshCommit, StorageError> {
        if nodes.is_empty() || nodes.len() > 100 {
            return Err(StorageError::SubscriptionInvalid);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut commit = apply_refresh_transaction(
            &transaction,
            source_id,
            expected_source_revision,
            expected_generation,
            content_hash,
            nodes,
            attempted_at_unix_ms,
            next_refresh_at_unix_ms,
        )?;
        transaction.commit()?;
        normalize_commit(&mut commit);
        Ok(commit)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_and_apply_refresh(
        &mut self,
        source: &SubscriptionSource,
        expected_current_revision: Option<u64>,
        expected_generation: u64,
        content_hash: [u8; 32],
        nodes: &[SubscriptionNode],
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<SubscriptionRefreshCommit, StorageError> {
        validate_node_count(nodes)?;
        if source.content_generation() != expected_generation {
            return Err(StorageError::SubscriptionGenerationConflict);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_source_revision(&transaction, source, expected_current_revision)?;
        save_source(&transaction, source, attempted_at_unix_ms)?;
        let mut commit = apply_refresh_transaction(
            &transaction,
            source.id(),
            source.revision(),
            expected_generation,
            content_hash,
            nodes,
            attempted_at_unix_ms,
            next_refresh_at_unix_ms,
        )?;
        transaction.commit()?;
        normalize_commit(&mut commit);
        Ok(commit)
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_refresh_transaction(
    transaction: &Transaction<'_>,
    source_id: &str,
    expected_source_revision: u64,
    expected_generation: u64,
    content_hash: [u8; 32],
    nodes: &[SubscriptionNode],
    attempted_at_unix_ms: u64,
    next_refresh_at_unix_ms: u64,
) -> Result<SubscriptionRefreshCommit, StorageError> {
    validate_node_count(nodes)?;
    let generation = validate_source_state(
        transaction,
        source_id,
        expected_source_revision,
        expected_generation,
    )?;
    let current = read_current_ownership(transaction, source_id)?;
    validate_nodes(transaction, source_id, nodes, &current)?;
    let outbounds = nodes
        .iter()
        .map(|node| (node.outbound().clone(), node.expected_revision()))
        .collect::<Vec<_>>();
    validate_default_outbound(transaction, &outbounds)?;

    let incoming = nodes
        .iter()
        .map(|node| node.outbound().id().as_str())
        .collect::<HashSet<_>>();
    let retired = current
        .values()
        .filter(|ownership| {
            ownership.present() && !incoming.contains(ownership.outbound_id().as_str())
        })
        .map(|ownership| ownership.outbound_id().clone())
        .collect::<Vec<_>>();
    reject_default_retirement(transaction, &retired)?;

    let mut replaced = Vec::new();
    for node in nodes {
        collect_replaced_credential(transaction, node, &mut replaced)?;
        save_outbound(transaction, node.outbound(), attempted_at_unix_ms)?;
        save_ownership(transaction, source_id, node, generation)?;
    }
    retire_missing_nodes(transaction, source_id, &retired, attempted_at_unix_ms)?;
    update_refresh_state(
        transaction,
        source_id,
        expected_generation,
        generation,
        content_hash,
        nodes.len(),
        attempted_at_unix_ms,
        next_refresh_at_unix_ms,
    )?;
    audit_refresh(
        transaction,
        source_id,
        generation,
        nodes.len(),
        attempted_at_unix_ms,
    )?;
    Ok(SubscriptionRefreshCommit::new(
        generation, replaced, retired,
    ))
}

fn validate_node_count(nodes: &[SubscriptionNode]) -> Result<(), StorageError> {
    if nodes.is_empty() || nodes.len() > 100 {
        return Err(StorageError::SubscriptionInvalid);
    }
    Ok(())
}

fn normalize_commit(commit: &mut SubscriptionRefreshCommit) {
    commit.normalize_replaced_credentials();
}

fn read_current_ownership(
    transaction: &Transaction<'_>,
    source_id: &str,
) -> Result<HashMap<String, SubscriptionNodeOwnership>, StorageError> {
    let mut statement = transaction.prepare(
        "SELECT outbound_id, node_key, present, last_seen_generation
         FROM subscription_outbound WHERE subscription_id = ?1",
    )?;
    let rows = statement.query_map([source_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (id, key, present, generation) = row?;
        let ownership = decode_ownership(id, key, present, generation)?;
        Ok((ownership.outbound_id().as_str().to_owned(), ownership))
    })
    .collect()
}

fn validate_nodes(
    transaction: &Transaction<'_>,
    source_id: &str,
    nodes: &[SubscriptionNode],
    current: &HashMap<String, SubscriptionNodeOwnership>,
) -> Result<(), StorageError> {
    let mut keys = HashSet::new();
    let mut ids = HashSet::new();
    for node in nodes {
        if !keys.insert(node.node_key()) || !ids.insert(node.outbound().id().as_str()) {
            return Err(StorageError::SubscriptionInvalid);
        }
        let owner = transaction
            .query_row(
                "SELECT subscription_id, node_key FROM subscription_outbound
                 WHERE outbound_id = ?1",
                [node.outbound().id().as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let outbound_exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM outbound WHERE id = ?1)",
            [node.outbound().id().as_str()],
            |row| row.get::<_, bool>(0),
        )?;
        let node_key_changed_owner = current.values().any(|ownership| {
            ownership.node_key() == node.node_key()
                && ownership.outbound_id() != node.outbound().id()
        });
        if owner
            .as_ref()
            .is_some_and(|(owner, key)| owner != source_id || key != node.node_key())
            || (owner.is_none() && outbound_exists)
            || node_key_changed_owner
        {
            return Err(StorageError::SubscriptionOwnershipConflict);
        }
        validate_revision(transaction, node.outbound(), node.expected_revision())?;
    }
    Ok(())
}

fn save_ownership(
    transaction: &Transaction<'_>,
    source_id: &str,
    node: &SubscriptionNode,
    generation: u64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO subscription_outbound(
             subscription_id, outbound_id, node_key, present, last_seen_generation
         ) VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(subscription_id, node_key) DO UPDATE SET
             outbound_id = excluded.outbound_id,
             present = 1,
             last_seen_generation = excluded.last_seen_generation",
        params![
            source_id,
            node.outbound().id().as_str(),
            node.node_key(),
            to_sqlite_u64(generation)?
        ],
    )?;
    Ok(())
}

fn reject_default_retirement(
    transaction: &Transaction<'_>,
    retired: &[OutboundId],
) -> Result<(), StorageError> {
    let default = transaction
        .query_row(
            "SELECT default_outbound_id FROM routing_settings
             WHERE singleton_id = 1 AND default_action = 'proxy'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if default.is_some_and(|value| retired.iter().any(|id| id.as_str() == value)) {
        return Err(StorageError::SubscriptionDefaultOutboundRemoved);
    }
    Ok(())
}

fn retire_missing_nodes(
    transaction: &Transaction<'_>,
    source_id: &str,
    retired: &[OutboundId],
    now: u64,
) -> Result<(), StorageError> {
    for id in retired {
        let changed = transaction.execute(
            "UPDATE outbound SET enabled = 0, revision = revision + 1,
                 updated_at_unix_ms = ?1
             WHERE id = ?2 AND revision < 9223372036854775807",
            params![to_sqlite_u64(now)?, id.as_str()],
        )?;
        if changed != 1 {
            return Err(StorageError::OutboundRevisionConflict);
        }
        transaction.execute(
            "UPDATE subscription_outbound SET present = 0
             WHERE subscription_id = ?1 AND outbound_id = ?2",
            params![source_id, id.as_str()],
        )?;
    }
    Ok(())
}

fn collect_replaced_credential(
    transaction: &Transaction<'_>,
    node: &SubscriptionNode,
    replaced: &mut Vec<String>,
) -> Result<(), StorageError> {
    let current = transaction
        .query_row(
            "SELECT credential_reference FROM outbound WHERE id = ?1",
            [node.outbound().id().as_str()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let next = node
        .outbound()
        .credential()
        .map(CredentialReference::item_reference);
    if current.as_deref().is_some_and(|value| Some(value) != next)
        && let Some(value) = current
    {
        replaced.push(value);
    }
    Ok(())
}
