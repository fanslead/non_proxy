use std::collections::BTreeSet;

use nonproxy_learning::{ConfirmationId, LearningSessionId};
use nonproxy_model::{DomainName, PolicyId};
use rusqlite::{Connection, OptionalExtension};

use crate::{LearningPolicySelection, StorageError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmedLearningPolicy {
    domain: DomainName,
    policy_id: PolicyId,
}

impl ConfirmedLearningPolicy {
    #[must_use]
    pub const fn domain(&self) -> &DomainName {
        &self.domain
    }

    #[must_use]
    pub const fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningConfirmationReceipt {
    confirmation_id: ConfirmationId,
    session_id: LearningSessionId,
    policies: Vec<ConfirmedLearningPolicy>,
    snapshot_version: Option<u64>,
    replayed: bool,
}

impl LearningConfirmationReceipt {
    #[must_use]
    pub const fn confirmation_id(&self) -> &ConfirmationId {
        &self.confirmation_id
    }

    #[must_use]
    pub const fn session_id(&self) -> &LearningSessionId {
        &self.session_id
    }

    #[must_use]
    pub fn policies(&self) -> &[ConfirmedLearningPolicy] {
        &self.policies
    }

    #[must_use]
    pub const fn snapshot_version(&self) -> Option<u64> {
        self.snapshot_version
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

pub(crate) fn validate_replay(
    receipt: &LearningConfirmationReceipt,
    session_id: &LearningSessionId,
    selections: &[LearningPolicySelection],
) -> Result<(), StorageError> {
    let expected = receipt
        .policies()
        .iter()
        .map(|value| (value.domain().as_ascii(), value.policy_id().as_str()))
        .collect::<BTreeSet<_>>();
    let actual = selections
        .iter()
        .map(|value| (value.domain().as_ascii(), value.policy().id().as_str()))
        .collect::<BTreeSet<_>>();
    if receipt.session_id() != session_id || expected != actual {
        return Err(StorageError::LearningConfirmationReplayMismatch);
    }
    Ok(())
}

pub(crate) fn load_receipt(
    connection: &Connection,
    confirmation_id: &ConfirmationId,
    replayed: bool,
) -> Result<Option<LearningConfirmationReceipt>, StorageError> {
    let header = connection
        .query_row(
            "SELECT session_id, snapshot_version
             FROM learning_confirmation WHERE id = ?1",
            [confirmation_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()?;
    let Some((session_id, snapshot_version)) = header else {
        return Ok(None);
    };
    let mut statement = connection.prepare(
        "SELECT candidate_key, policy_id
         FROM learning_candidate_decision
         WHERE confirmation_id = ?1 AND selected = 1
         ORDER BY candidate_key",
    )?;
    let policies = statement
        .query_map([confirmation_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|(domain, policy_id)| {
            Ok(ConfirmedLearningPolicy {
                domain: DomainName::normalize(&domain)?,
                policy_id: PolicyId::new(policy_id)?,
            })
        })
        .collect::<Result<Vec<_>, StorageError>>()?;
    Ok(Some(LearningConfirmationReceipt {
        confirmation_id: ConfirmationId::new(confirmation_id.as_str().to_owned())?,
        session_id: LearningSessionId::new(session_id)?,
        policies,
        snapshot_version: snapshot_version
            .map(|value| {
                u64::try_from(value).map_err(|_| StorageError::CorruptData {
                    field: "learning_confirmation.snapshot_version",
                })
            })
            .transpose()?,
        replayed,
    }))
}
