use std::collections::{BTreeMap, BTreeSet};

use nonproxy_learning::{ConfirmationId, LearningSessionId, LearningSessionState};
use nonproxy_model::{
    DomainMatchKind, DomainName, Policy, PolicyId, PolicySourceKind, RouteAction,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::{
    StorageError,
    learning_codec::{query_candidates, query_session},
    learning_confirmation_receipt::{LearningConfirmationReceipt, load_receipt, validate_replay},
    learning_repository::expire_active,
    migration::to_sqlite_u64,
    policy_repository::{
        increment_catalog_generation, insert_policy_audit, load_policy, replace_matcher_children,
        upsert_policy, validate_revision,
    },
};

pub struct LearningPolicySelection {
    domain: DomainName,
    policy: Policy,
    existing: bool,
}

impl LearningPolicySelection {
    #[must_use]
    pub const fn new(domain: DomainName, policy: Policy, existing: bool) -> Self {
        Self {
            domain,
            policy,
            existing,
        }
    }

    #[must_use]
    pub const fn domain(&self) -> &DomainName {
        &self.domain
    }

    #[must_use]
    pub const fn policy(&self) -> &Policy {
        &self.policy
    }

    #[must_use]
    pub const fn existing(&self) -> bool {
        self.existing
    }
}

pub struct LearningConfirmationRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> LearningConfirmationRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn get(
        &self,
        confirmation_id: &ConfirmationId,
    ) -> Result<Option<LearningConfirmationReceipt>, StorageError> {
        load_receipt(self.connection, confirmation_id, false)
    }

    pub fn confirm_site(
        &mut self,
        confirmation_id: &ConfirmationId,
        session_id: &LearningSessionId,
        selections: &[LearningPolicySelection],
        confirmed_at_unix_ms: u64,
    ) -> Result<LearningConfirmationReceipt, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(receipt) = load_receipt(&transaction, confirmation_id, true)? {
            validate_replay(&receipt, session_id, selections)?;
            transaction.commit()?;
            return Ok(receipt);
        }
        if confirmation_exists_for_session(&transaction, session_id)? {
            return Err(StorageError::LearningSessionAlreadyConfirmed);
        }

        expire_active(&transaction, confirmed_at_unix_ms)?;
        let session = query_session(&transaction, session_id)?
            .ok_or(StorageError::LearningSessionNotFound)?;
        if session.state() == LearningSessionState::Active {
            return Err(StorageError::LearningSessionStillActive);
        }
        let Some(site) = session.subject().site() else {
            return Err(StorageError::LearningConfirmationInvalid);
        };
        let candidates = query_candidates(&transaction, session_id)?;
        let candidate_domains = candidates
            .iter()
            .map(|value| value.domain().as_ascii())
            .collect::<BTreeSet<_>>();
        let selected = validate_selections(selections, &candidate_domains, site, &transaction)?;

        for selection in selections {
            if selection.existing {
                continue;
            }
            validate_revision(&transaction, selection.policy(), None)?;
            upsert_policy(&transaction, selection.policy(), confirmed_at_unix_ms)?;
            replace_matcher_children(&transaction, selection.policy())?;
            insert_policy_audit(
                &transaction,
                "learning_policy_saved",
                selection.policy(),
                confirmed_at_unix_ms,
            )?;
        }
        if selections.iter().any(|value| !value.existing) {
            increment_catalog_generation(&transaction)?;
        }

        transaction.execute(
            "INSERT INTO learning_confirmation(
                 id, session_id, confirmed_at_unix_ms, selected_count,
                 snapshot_version
             ) VALUES (?1, ?2, ?3, ?4, NULL)",
            params![
                confirmation_id.as_str(),
                session_id.as_str(),
                to_sqlite_u64(confirmed_at_unix_ms)?,
                i64::try_from(selected.len())
                    .map_err(|_| { StorageError::LearningConfirmationInvalid })?,
            ],
        )?;
        for candidate in candidates {
            let policy_id = selected.get(candidate.domain().as_ascii());
            transaction.execute(
                "INSERT INTO learning_candidate_decision(
                     confirmation_id, session_id, candidate_key, selected,
                     policy_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    confirmation_id.as_str(),
                    session_id.as_str(),
                    candidate.domain().as_ascii(),
                    policy_id.is_some(),
                    policy_id.map(|value| value.as_str()),
                ],
            )?;
        }
        transaction.execute(
            "INSERT INTO audit_event(
                 event_id, event_type, occurred_at_unix_ms, details
             ) VALUES (?1, 'learning_candidates_confirmed', ?2, ?3)",
            params![
                format!("learning_confirmed:{}", confirmation_id.as_str()),
                to_sqlite_u64(confirmed_at_unix_ms)?,
                format!(
                    "session_id={};selected_count={}",
                    session_id.as_str(),
                    selected.len()
                ),
            ],
        )?;
        transaction.commit()?;

        load_receipt(self.connection, confirmation_id, false)?.ok_or(StorageError::CorruptData {
            field: "learning_confirmation.id",
        })
    }

    pub fn mark_snapshot(
        &mut self,
        confirmation_id: &ConfirmationId,
        snapshot_version: u64,
    ) -> Result<(), StorageError> {
        let snapshot = to_sqlite_u64(snapshot_version)?;
        let current = self
            .connection
            .query_row(
                "SELECT snapshot_version FROM learning_confirmation
                 WHERE id = ?1",
                [confirmation_id.as_str()],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?;
        match current {
            None => Err(StorageError::LearningConfirmationInvalid),
            Some(Some(value)) if value != snapshot => {
                Err(StorageError::LearningConfirmationReplayMismatch)
            }
            Some(Some(_)) => Ok(()),
            Some(None) => {
                let changed = self.connection.execute(
                    "UPDATE learning_confirmation
                     SET snapshot_version = ?2
                     WHERE id = ?1 AND snapshot_version IS NULL",
                    params![confirmation_id.as_str(), snapshot],
                )?;
                if changed == 1 {
                    Ok(())
                } else {
                    Err(StorageError::LearningConfirmationReplayMismatch)
                }
            }
        }
    }
}

fn validate_selections<'a>(
    selections: &'a [LearningPolicySelection],
    candidate_domains: &BTreeSet<&str>,
    site: &DomainName,
    transaction: &Transaction<'_>,
) -> Result<BTreeMap<&'a str, &'a PolicyId>, StorageError> {
    if selections.is_empty() || selections.len() > 256 {
        return Err(StorageError::LearningConfirmationInvalid);
    }
    let mut selected = BTreeMap::new();
    for selection in selections {
        validate_selection(selection, transaction)?;
        let domain = selection.domain().as_ascii();
        if !candidate_domains.contains(domain)
            || selected.insert(domain, selection.policy().id()).is_some()
        {
            return Err(StorageError::LearningConfirmationInvalid);
        }
    }
    if !selected.contains_key(site.as_ascii()) {
        return Err(StorageError::LearningConfirmationInvalid);
    }
    Ok(selected)
}

fn validate_selection(
    selection: &LearningPolicySelection,
    transaction: &Transaction<'_>,
) -> Result<(), StorageError> {
    let policy = selection.policy();
    let matcher = policy.matcher().domain();
    if policy.source_kind() != PolicySourceKind::Site
        || policy.decision().action() != RouteAction::Direct
        || !policy.enabled()
        || policy.matcher().app().is_some()
        || policy.matcher().cidr().is_some()
        || policy.matcher().network().is_some()
        || !policy.matcher().transports().is_empty()
        || !policy.matcher().ports().is_empty()
        || matcher.is_none_or(|value| {
            value.kind() != DomainMatchKind::Exact || value.pattern() != selection.domain()
        })
    {
        return Err(StorageError::LearningConfirmationInvalid);
    }
    let stored = load_policy(transaction, policy.id())?;
    match (selection.existing, stored) {
        (true, Some(value)) if value == *policy => Ok(()),
        (false, None) => Ok(()),
        _ => Err(StorageError::LearningConfirmationInvalid),
    }
}

fn confirmation_exists_for_session(
    connection: &Connection,
    session_id: &LearningSessionId,
) -> Result<bool, StorageError> {
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM learning_confirmation WHERE session_id = ?1
             )",
            [session_id.as_str()],
            |row| row.get(0),
        )
        .map_err(StorageError::from)
}
