use nonproxy_learning::{
    BrowserContextId, LearningCandidate, LearningObservation, LearningSession, LearningSessionId,
    LearningSessionState, classify,
};
use rusqlite::{
    Connection, ErrorCode, OptionalExtension, Transaction, TransactionBehavior, params,
};

use crate::{
    StorageError,
    learning_codec::{
        app_columns, query_candidate, query_candidates, query_session, save_candidate,
        u32_from_sqlite,
    },
    migration::to_sqlite_u64,
};

pub const MAX_LEARNING_CANDIDATES: u32 = 256;
pub const MAX_LEARNING_OBSERVATIONS: u32 = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningObservationResult {
    candidate: LearningCandidate,
    duplicate: bool,
}

impl LearningObservationResult {
    #[must_use]
    pub const fn candidate(&self) -> &LearningCandidate {
        &self.candidate
    }

    #[must_use]
    pub const fn duplicate(&self) -> bool {
        self.duplicate
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoppedLearning {
    session: LearningSession,
    candidate_count: u32,
}

impl StoppedLearning {
    #[must_use]
    pub const fn session(&self) -> &LearningSession {
        &self.session
    }

    #[must_use]
    pub const fn candidate_count(&self) -> u32 {
        self.candidate_count
    }
}

pub struct LearningRepository<'connection> {
    connection: &'connection mut Connection,
}

impl<'connection> LearningRepository<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection) -> Self {
        Self { connection }
    }

    pub fn start(&mut self, session: &LearningSession) -> Result<(), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        expire_active(&transaction, session.started_at_unix_ms())?;
        let (app_platform, app_signer) = app_columns(session.subject());
        let result = transaction.execute(
            "INSERT INTO learning_session(
                 id, kind, target, app_platform, app_signer_id, browser_context_id,
                 state, started_at_unix_ms, expires_at_unix_ms, stopped_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8, NULL)",
            params![
                session.id().as_str(),
                session.subject().kind().as_str(),
                session.subject().target(),
                app_platform,
                app_signer,
                session.browser_context_id().map(BrowserContextId::as_str),
                to_sqlite_u64(session.started_at_unix_ms())?,
                to_sqlite_u64(session.expires_at_unix_ms())?,
            ],
        );
        match result {
            Ok(_) => transaction.commit().map_err(StorageError::from),
            Err(error) if is_constraint(&error) => Err(StorageError::ActiveLearningSessionExists),
            Err(error) => Err(error.into()),
        }
    }

    pub fn get(
        &mut self,
        session_id: &LearningSessionId,
        now_unix_ms: u64,
    ) -> Result<Option<LearningSession>, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        expire_active(&transaction, now_unix_ms)?;
        let session = query_session(&transaction, session_id)?;
        transaction.commit()?;
        Ok(session)
    }

    pub fn list_candidates(
        &mut self,
        session_id: &LearningSessionId,
        now_unix_ms: u64,
    ) -> Result<(LearningSession, Vec<LearningCandidate>), StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        expire_active(&transaction, now_unix_ms)?;
        let session = query_session(&transaction, session_id)?
            .ok_or(StorageError::LearningSessionNotFound)?;
        let candidates = query_candidates(&transaction, session_id)?;
        transaction.commit()?;
        Ok((session, candidates))
    }

    pub fn stop(
        &mut self,
        session_id: &LearningSessionId,
        now_unix_ms: u64,
    ) -> Result<StoppedLearning, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        expire_active(&transaction, now_unix_ms)?;
        let mut session = query_session(&transaction, session_id)?
            .ok_or(StorageError::LearningSessionNotFound)?;
        if session.state() == LearningSessionState::Active {
            transaction.execute(
                "UPDATE learning_session
                 SET state = 'stopped', stopped_at_unix_ms = ?2
                 WHERE id = ?1 AND state = 'active'",
                params![session_id.as_str(), to_sqlite_u64(now_unix_ms)?],
            )?;
            session = query_session(&transaction, session_id)?
                .ok_or(StorageError::LearningSessionNotFound)?;
        }
        let candidate_count = count_candidates(&transaction, session_id)?;
        transaction.commit()?;
        Ok(StoppedLearning {
            session,
            candidate_count,
        })
    }

    pub fn record_observation(
        &mut self,
        observation: &LearningObservation,
        now_unix_ms: u64,
    ) -> Result<LearningObservationResult, StorageError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        expire_active(&transaction, now_unix_ms)?;
        let session = query_session(&transaction, observation.session_id())?
            .ok_or(StorageError::LearningSessionNotFound)?;
        session.validate_observation(observation.browser_context_id(), now_unix_ms)?;

        if let Some(candidate_key) = receipt_candidate(&transaction, observation)? {
            let candidate =
                query_candidate(&transaction, observation.session_id(), &candidate_key)?.ok_or(
                    StorageError::CorruptData {
                        field: "learning_observation_receipt.candidate_key",
                    },
                )?;
            transaction.commit()?;
            return Ok(LearningObservationResult {
                candidate,
                duplicate: true,
            });
        }
        enforce_observation_limit(&transaction, observation.session_id())?;

        let previous = query_candidate(
            &transaction,
            observation.session_id(),
            observation.domain().as_ascii(),
        )?;
        if previous.is_none()
            && count_candidates(&transaction, observation.session_id())? >= MAX_LEARNING_CANDIDATES
        {
            return Err(StorageError::LearningCandidateLimitReached);
        }
        let candidate = classify(&session, observation, previous.as_ref(), now_unix_ms);
        save_candidate(&transaction, observation.session_id(), &candidate)?;
        transaction.execute(
            "INSERT INTO learning_observation_receipt(
                 session_id, observation_id, candidate_key, observed_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                observation.session_id().as_str(),
                observation.observation_id().as_str(),
                candidate.domain().as_ascii(),
                to_sqlite_u64(now_unix_ms)?,
            ],
        )?;
        transaction.commit()?;
        Ok(LearningObservationResult {
            candidate,
            duplicate: false,
        })
    }
}

fn expire_active(transaction: &Transaction<'_>, now_unix_ms: u64) -> Result<(), StorageError> {
    transaction.execute(
        "UPDATE learning_session
         SET state = 'expired', stopped_at_unix_ms = expires_at_unix_ms
         WHERE state = 'active' AND expires_at_unix_ms <= ?1",
        [to_sqlite_u64(now_unix_ms)?],
    )?;
    Ok(())
}

fn receipt_candidate(
    transaction: &Transaction<'_>,
    observation: &LearningObservation,
) -> Result<Option<String>, StorageError> {
    transaction
        .query_row(
            "SELECT candidate_key FROM learning_observation_receipt
             WHERE session_id = ?1 AND observation_id = ?2",
            params![
                observation.session_id().as_str(),
                observation.observation_id().as_str()
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::from)
}

fn enforce_observation_limit(
    transaction: &Transaction<'_>,
    session_id: &LearningSessionId,
) -> Result<(), StorageError> {
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM learning_observation_receipt WHERE session_id = ?1",
        [session_id.as_str()],
        |row| row.get(0),
    )?;
    if u32_from_sqlite(count, "learning_observation_receipt.count")? >= MAX_LEARNING_OBSERVATIONS {
        return Err(StorageError::LearningObservationLimitReached);
    }
    Ok(())
}

fn count_candidates(
    connection: &Connection,
    session_id: &LearningSessionId,
) -> Result<u32, StorageError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM learning_candidate WHERE session_id = ?1",
        [session_id.as_str()],
        |row| row.get(0),
    )?;
    u32_from_sqlite(count, "learning_candidate.count")
}

fn is_constraint(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(value, _)
            if value.code == ErrorCode::ConstraintViolation
    )
}
