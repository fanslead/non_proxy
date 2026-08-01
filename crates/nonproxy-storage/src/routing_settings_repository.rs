use nonproxy_model::OutboundId;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    OutboundKind, SnapshotArtifact, StorageError,
    migration::to_sqlite_u64,
    snapshot_query::ensure_active_snapshot_version,
    snapshot_repository::{
        stage_in_transaction, stage_rebuilt_rollback_in_transaction, stage_rollback_in_transaction,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefaultRoute {
    Direct,
    Proxy(OutboundId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingSettings {
    route: DefaultRoute,
    revision: u64,
}

impl RoutingSettings {
    #[must_use]
    pub const fn route(&self) -> &DefaultRoute {
        &self.route
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

pub struct RoutingSettingsRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> RoutingSettingsRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn get(&self) -> Result<RoutingSettings, StorageError> {
        read_settings(self.connection)
    }

    pub fn set_and_stage(
        &mut self,
        route: &DefaultRoute,
        expected_revision: u64,
        artifact: &SnapshotArtifact,
        updated_at_unix_ms: u64,
    ) -> Result<RoutingSettings, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let settings = update_route(&transaction, route, expected_revision, updated_at_unix_ms)?;
        stage_in_transaction(&transaction, artifact)?;
        transaction.commit()?;
        Ok(settings)
    }

    pub fn set_and_stage_rollback(
        &mut self,
        route: &DefaultRoute,
        expected_revision: u64,
        new_snapshot_version: u64,
        source_snapshot_version: u64,
        updated_at_unix_ms: u64,
    ) -> Result<(RoutingSettings, SnapshotArtifact), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let settings = update_route(&transaction, route, expected_revision, updated_at_unix_ms)?;
        let artifact = stage_rollback_in_transaction(
            &transaction,
            new_snapshot_version,
            source_snapshot_version,
            updated_at_unix_ms,
        )?;
        transaction.commit()?;
        Ok((settings, artifact))
    }

    pub fn set_and_stage_rebuilt_rollback(
        &mut self,
        route: &DefaultRoute,
        expected_revision: u64,
        artifact: &SnapshotArtifact,
        source_snapshot_version: u64,
        expected_active_snapshot_version: u64,
        updated_at_unix_ms: u64,
    ) -> Result<RoutingSettings, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_active_snapshot_version(&transaction, expected_active_snapshot_version)?;
        let settings = update_route(&transaction, route, expected_revision, updated_at_unix_ms)?;
        stage_rebuilt_rollback_in_transaction(&transaction, artifact, source_snapshot_version)?;
        transaction.commit()?;
        Ok(settings)
    }
}

fn read_settings(connection: &Connection) -> Result<RoutingSettings, StorageError> {
    let raw = connection
        .query_row(
            "SELECT default_action, default_outbound_id, revision
             FROM routing_settings WHERE singleton_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or(StorageError::CorruptData {
            field: "routing_settings.singleton_id",
        })?;
    decode_settings(raw)
}

fn decode_settings(
    (action, outbound_id, revision): (String, Option<String>, i64),
) -> Result<RoutingSettings, StorageError> {
    let route = match (action.as_str(), outbound_id) {
        ("direct", None) => DefaultRoute::Direct,
        ("proxy", Some(outbound_id)) => DefaultRoute::Proxy(OutboundId::new(outbound_id)?),
        _ => {
            return Err(StorageError::CorruptData {
                field: "routing_settings.default_action",
            });
        }
    };
    let revision = u64::try_from(revision).map_err(|_| StorageError::CorruptData {
        field: "routing_settings.revision",
    })?;
    if revision == 0 {
        return Err(StorageError::CorruptData {
            field: "routing_settings.revision",
        });
    }
    Ok(RoutingSettings { route, revision })
}

fn update_route(
    transaction: &Transaction<'_>,
    route: &DefaultRoute,
    expected_revision: u64,
    updated_at_unix_ms: u64,
) -> Result<RoutingSettings, StorageError> {
    let current = read_settings(transaction)?;
    if expected_revision == 0 || current.revision() != expected_revision {
        return Err(StorageError::RoutingRevisionConflict);
    }
    if let DefaultRoute::Proxy(outbound_id) = route {
        let outbound = transaction
            .query_row(
                "SELECT enabled, kind FROM outbound WHERE id = ?1",
                [outbound_id.as_str()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let available = match outbound {
            Some((1, kind)) => OutboundKind::parse(&kind)?.supports_default_route(),
            _ => false,
        };
        if !available {
            return Err(StorageError::DefaultOutboundUnavailable);
        }
    }
    let revision = expected_revision
        .checked_add(1)
        .ok_or(StorageError::RoutingRevisionConflict)?;
    let (action, outbound_id) = match route {
        DefaultRoute::Direct => ("direct", None),
        DefaultRoute::Proxy(outbound_id) => ("proxy", Some(outbound_id.as_str())),
    };
    let changed = transaction.execute(
        "UPDATE routing_settings
         SET default_action = ?1, default_outbound_id = ?2,
             revision = ?3, updated_at_unix_ms = ?4
         WHERE singleton_id = 1 AND revision = ?5",
        params![
            action,
            outbound_id,
            to_sqlite_u64(revision)?,
            to_sqlite_u64(updated_at_unix_ms)?,
            to_sqlite_u64(expected_revision)?
        ],
    )?;
    if changed != 1 {
        return Err(StorageError::RoutingRevisionConflict);
    }
    Ok(RoutingSettings {
        route: route.clone(),
        revision,
    })
}
