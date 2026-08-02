use nonproxy_model::{Policy, PolicyId};
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, named_params, params,
};

use crate::{
    StorageError,
    migration::to_sqlite_u64,
    policy_codec::{
        RawPolicy, action_code, decode_policy, domain_kind_code, failure_mode_code, origin_code,
        platform_code, source_code, transport_code,
    },
};

const POLICY_SELECT: &str = "
    SELECT id, display_name, source_kind, decision_action, outbound_id,
           failure_mode, priority, enabled, origin, revision,
           app_platform, app_stable_id, app_signer_id, app_include_helpers,
           cidr, network_profile_id, outbound_group_id
    FROM policy";

pub struct PolicyRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> PolicyRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn save(
        &mut self,
        policy: &Policy,
        expected_current_revision: Option<u64>,
        updated_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_revision(&transaction, policy, expected_current_revision)?;
        upsert_policy(&transaction, policy, updated_at_unix_ms)?;
        replace_matcher_children(&transaction, policy)?;
        insert_policy_audit(&transaction, "policy_saved", policy, updated_at_unix_ms)?;
        increment_catalog_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get(&self, policy_id: &PolicyId) -> Result<Option<Policy>, StorageError> {
        load_policy(self.connection, policy_id)
    }

    pub fn list(&self) -> Result<Vec<Policy>, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM policy ORDER BY id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        ids.into_iter()
            .map(|id| {
                let id = PolicyId::new(id)?;
                load_policy(self.connection, &id)?
                    .ok_or(StorageError::CorruptData { field: "policy.id" })
            })
            .collect()
    }

    pub fn delete(
        &mut self,
        policy_id: &PolicyId,
        expected_current_revision: u64,
        deleted_at_unix_ms: u64,
    ) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision = transaction
            .query_row(
                "SELECT revision FROM policy WHERE id = ?1",
                [policy_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if revision != Some(to_sqlite_u64(expected_current_revision)?) {
            return Err(StorageError::PolicyRevisionConflict);
        }
        let changed = transaction.execute(
            "DELETE FROM policy WHERE id = ?1 AND revision = ?2",
            params![
                policy_id.as_str(),
                to_sqlite_u64(expected_current_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::PolicyRevisionConflict);
        }
        transaction.execute(
            "INSERT INTO audit_event(
                event_id, event_type, occurred_at_unix_ms, policy_id
             ) VALUES (?1, 'policy_deleted', ?2, ?3)",
            params![
                format!("policy_deleted:{}:{deleted_at_unix_ms}", policy_id.as_str()),
                to_sqlite_u64(deleted_at_unix_ms)?,
                policy_id.as_str()
            ],
        )?;
        increment_catalog_generation(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn catalog_generation(&self) -> Result<u64, StorageError> {
        let value: i64 = self.connection.query_row(
            "SELECT value FROM control_generation WHERE name = 'policy_catalog'",
            [],
            |row| row.get(0),
        )?;
        u64::try_from(value).map_err(|_| StorageError::CorruptData {
            field: "control_generation.value",
        })
    }
}

pub(crate) fn increment_catalog_generation(
    transaction: &Transaction<'_>,
) -> Result<(), StorageError> {
    let changed = transaction.execute(
        "UPDATE control_generation
         SET value = value + 1
         WHERE name = 'policy_catalog'",
        [],
    )?;
    if changed != 1 {
        return Err(StorageError::CorruptData {
            field: "control_generation.policy_catalog",
        });
    }
    Ok(())
}

pub(crate) fn validate_revision(
    transaction: &Transaction<'_>,
    policy: &Policy,
    expected_current_revision: Option<u64>,
) -> Result<(), StorageError> {
    let current = transaction
        .query_row(
            "SELECT revision FROM policy WHERE id = ?1",
            [policy.id().as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| {
            u64::try_from(value).map_err(|_| StorageError::CorruptData {
                field: "policy.revision",
            })
        })
        .transpose()?;
    let valid = match (current, expected_current_revision) {
        (None, None) => policy.revision() == 1,
        (Some(current), Some(expected)) => {
            current == expected && current.checked_add(1) == Some(policy.revision())
        }
        _ => false,
    };
    if !valid {
        return Err(StorageError::PolicyRevisionConflict);
    }
    Ok(())
}

pub(crate) fn upsert_policy(
    transaction: &Transaction<'_>,
    policy: &Policy,
    updated_at_unix_ms: u64,
) -> Result<(), StorageError> {
    let app = policy.matcher().app();
    transaction.execute(
        "INSERT INTO policy(
            id, display_name, source_kind, decision_action, outbound_id, outbound_group_id,
            failure_mode, priority, enabled, origin, revision,
            app_platform, app_stable_id, app_signer_id, app_include_helpers,
            cidr, network_profile_id, updated_at_unix_ms
         ) VALUES (
            :id, :display_name, :source_kind, :decision_action, :outbound_id, :outbound_group_id,
            :failure_mode, :priority, :enabled, :origin, :revision,
            :app_platform, :app_stable_id, :app_signer_id,
            :app_include_helpers, :cidr, :network_profile_id, :updated_at
         )
         ON CONFLICT(id) DO UPDATE SET
            display_name = excluded.display_name,
            source_kind = excluded.source_kind,
            decision_action = excluded.decision_action,
            outbound_id = excluded.outbound_id,
            outbound_group_id = excluded.outbound_group_id,
            failure_mode = excluded.failure_mode,
            priority = excluded.priority,
            enabled = excluded.enabled,
            origin = excluded.origin,
            revision = excluded.revision,
            app_platform = excluded.app_platform,
            app_stable_id = excluded.app_stable_id,
            app_signer_id = excluded.app_signer_id,
            app_include_helpers = excluded.app_include_helpers,
            cidr = excluded.cidr,
            network_profile_id = excluded.network_profile_id,
            updated_at_unix_ms = excluded.updated_at_unix_ms",
        named_params! {
            ":id": policy.id().as_str(),
            ":display_name": policy.display_name(),
            ":source_kind": source_code(policy.source_kind()),
            ":decision_action": action_code(policy.decision().action()),
            ":outbound_id": policy.decision().outbound_id().map(|value| value.as_str()),
            ":outbound_group_id": policy
                .decision()
                .outbound_group_id()
                .map(|value| value.as_str()),
            ":failure_mode": failure_mode_code(policy.decision().failure_mode()),
            ":priority": i64::from(policy.priority()),
            ":enabled": i64::from(policy.enabled()),
            ":origin": origin_code(policy.origin()),
            ":revision": to_sqlite_u64(policy.revision())?,
            ":app_platform": app.map(|value| platform_code(value.platform())),
            ":app_stable_id": app.map(|value| value.stable_id()),
            ":app_signer_id": app.and_then(|value| value.signer_id()),
            ":app_include_helpers": app.map(|value| i64::from(value.includes_helpers())),
            ":cidr": policy.matcher().cidr().map(|value| value.to_string()),
            ":network_profile_id": policy.matcher().network().map(|value| value.profile_id().as_str()),
            ":updated_at": to_sqlite_u64(updated_at_unix_ms)?
        },
    )?;
    Ok(())
}

pub(crate) fn replace_matcher_children(
    transaction: &Transaction<'_>,
    policy: &Policy,
) -> Result<(), StorageError> {
    transaction.execute(
        "DELETE FROM domain_target WHERE policy_id = ?1",
        [policy.id().as_str()],
    )?;
    transaction.execute(
        "DELETE FROM policy_transport WHERE policy_id = ?1",
        [policy.id().as_str()],
    )?;
    transaction.execute(
        "DELETE FROM policy_port_range WHERE policy_id = ?1",
        [policy.id().as_str()],
    )?;
    if let Some(domain) = policy.matcher().domain() {
        transaction.execute(
            "INSERT INTO domain_target(policy_id, match_kind, ascii_pattern)
             VALUES (?1, ?2, ?3)",
            params![
                policy.id().as_str(),
                domain_kind_code(domain.kind()),
                domain.pattern().as_ascii()
            ],
        )?;
    }
    for transport in policy.matcher().transports() {
        transaction.execute(
            "INSERT INTO policy_transport(policy_id, transport)
             VALUES (?1, ?2)",
            params![policy.id().as_str(), transport_code(*transport)],
        )?;
    }
    for range in policy.matcher().ports() {
        transaction.execute(
            "INSERT INTO policy_port_range(
                policy_id, first_port, last_port
             ) VALUES (?1, ?2, ?3)",
            params![
                policy.id().as_str(),
                i64::from(range.first()),
                i64::from(range.last())
            ],
        )?;
    }
    Ok(())
}

pub(crate) fn load_policy(
    connection: &Connection,
    policy_id: &PolicyId,
) -> Result<Option<Policy>, StorageError> {
    let raw = connection
        .query_row(
            &format!("{POLICY_SELECT} WHERE id = ?1"),
            [policy_id.as_str()],
            RawPolicy::from_row,
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let domain = connection
        .query_row(
            "SELECT match_kind, ascii_pattern
             FROM domain_target WHERE policy_id = ?1",
            [policy_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let transports = query_i64_values(
        connection,
        "SELECT transport FROM policy_transport
         WHERE policy_id = ?1 ORDER BY transport",
        policy_id,
    )?;
    let mut statement = connection.prepare(
        "SELECT first_port, last_port FROM policy_port_range
         WHERE policy_id = ?1 ORDER BY first_port, last_port",
    )?;
    let ports = statement
        .query_map([policy_id.as_str()], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    decode_policy(raw, domain, transports, ports).map(Some)
}

fn query_i64_values(
    connection: &Connection,
    sql: &str,
    policy_id: &PolicyId,
) -> Result<Vec<i64>, StorageError> {
    let mut statement = connection.prepare(sql)?;
    statement
        .query_map([policy_id.as_str()], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

pub(crate) fn insert_policy_audit(
    transaction: &Transaction<'_>,
    event_type: &str,
    policy: &Policy,
    occurred_at_unix_ms: u64,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO audit_event(
            event_id, event_type, occurred_at_unix_ms, policy_id, details
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            format!(
                "{event_type}:{}:r{}",
                policy.id().as_str(),
                policy.revision()
            ),
            event_type,
            to_sqlite_u64(occurred_at_unix_ms)?,
            policy.id().as_str(),
            format!("revision={}", policy.revision())
        ],
    )?;
    Ok(())
}
