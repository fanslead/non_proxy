use nonproxy_learning::{
    AppLearningSubject, BrowserContextId, LearningCandidate, LearningCandidateKind,
    LearningSession, LearningSessionId, LearningSessionKind, LearningSessionState, LearningSubject,
};
use nonproxy_model::{AppIdentity, DomainName, Platform};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};

use crate::{StorageError, migration::to_sqlite_u64};

pub(crate) fn query_session(
    connection: &Connection,
    session_id: &LearningSessionId,
) -> Result<Option<LearningSession>, StorageError> {
    connection
        .query_row(
            "SELECT id, kind, target, app_platform, app_signer_id, browser_context_id,
                    state, started_at_unix_ms, expires_at_unix_ms, stopped_at_unix_ms
             FROM learning_session WHERE id = ?1",
            [session_id.as_str()],
            session_from_row,
        )
        .optional()?
        .transpose()
}

pub(crate) fn query_candidates(
    connection: &Connection,
    session_id: &LearningSessionId,
) -> Result<Vec<LearningCandidate>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT candidate_key, classification, confidence_millis, requires_confirmation,
                evidence_count, first_seen_at_unix_ms, last_seen_at_unix_ms,
                main_frame_count, subresource_count, redirect_count
         FROM learning_candidate WHERE session_id = ?1
         ORDER BY requires_confirmation, confidence_millis DESC, candidate_key",
    )?;
    let rows = statement.query_map([session_id.as_str()], candidate_from_row)?;
    rows.collect::<Result<Vec<_>, _>>()?.into_iter().collect()
}

pub(crate) fn query_candidate(
    connection: &Connection,
    session_id: &LearningSessionId,
    candidate_key: &str,
) -> Result<Option<LearningCandidate>, StorageError> {
    connection
        .query_row(
            "SELECT candidate_key, classification, confidence_millis, requires_confirmation,
                    evidence_count, first_seen_at_unix_ms, last_seen_at_unix_ms,
                    main_frame_count, subresource_count, redirect_count
             FROM learning_candidate WHERE session_id = ?1 AND candidate_key = ?2",
            params![session_id.as_str(), candidate_key],
            candidate_from_row,
        )
        .optional()?
        .transpose()
}

pub(crate) fn save_candidate(
    transaction: &Transaction<'_>,
    session_id: &LearningSessionId,
    candidate: &LearningCandidate,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO learning_candidate(
             session_id, candidate_key, classification, confidence_millis,
             requires_confirmation, evidence_count, first_seen_at_unix_ms,
             last_seen_at_unix_ms, main_frame_count, subresource_count, redirect_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(session_id, candidate_key) DO UPDATE SET
             classification = excluded.classification,
             confidence_millis = excluded.confidence_millis,
             requires_confirmation = excluded.requires_confirmation,
             evidence_count = excluded.evidence_count,
             last_seen_at_unix_ms = excluded.last_seen_at_unix_ms,
             main_frame_count = excluded.main_frame_count,
             subresource_count = excluded.subresource_count,
             redirect_count = excluded.redirect_count",
        params![
            session_id.as_str(),
            candidate.domain().as_ascii(),
            candidate.kind().as_str(),
            i64::from(candidate.confidence_millis()),
            candidate.requires_confirmation(),
            i64::from(candidate.evidence_count()),
            to_sqlite_u64(candidate.first_seen_at_unix_ms())?,
            to_sqlite_u64(candidate.last_seen_at_unix_ms())?,
            i64::from(candidate.main_frame_count()),
            i64::from(candidate.subresource_count()),
            i64::from(candidate.redirect_count()),
        ],
    )?;
    Ok(())
}

pub(crate) fn app_columns(subject: &LearningSubject) -> (Option<i64>, Option<&str>) {
    match subject {
        LearningSubject::App(app) => {
            let platform = match app.platform() {
                Platform::MacOs => 1,
                Platform::Windows => 2,
            };
            (Some(platform), app.signer_id())
        }
        LearningSubject::Site(_) => (None, None),
    }
}

pub(crate) fn u32_from_sqlite(value: i64, field: &'static str) -> Result<u32, StorageError> {
    u32::try_from(value).map_err(|_| StorageError::CorruptData { field })
}

fn session_from_row(row: &Row<'_>) -> rusqlite::Result<Result<LearningSession, StorageError>> {
    let id: String = row.get(0)?;
    let kind: String = row.get(1)?;
    let target: String = row.get(2)?;
    let app_platform: Option<i64> = row.get(3)?;
    let app_signer: Option<String> = row.get(4)?;
    let browser_context: Option<String> = row.get(5)?;
    let state: String = row.get(6)?;
    let started: i64 = row.get(7)?;
    let expires: i64 = row.get(8)?;
    let stopped: Option<i64> = row.get(9)?;
    Ok(build_session(
        id,
        kind,
        target,
        app_platform,
        app_signer,
        browser_context,
        state,
        started,
        expires,
        stopped,
    ))
}

#[allow(clippy::too_many_arguments)]
fn build_session(
    id: String,
    kind: String,
    target: String,
    app_platform: Option<i64>,
    app_signer: Option<String>,
    browser_context: Option<String>,
    state: String,
    started: i64,
    expires: i64,
    stopped: Option<i64>,
) -> Result<LearningSession, StorageError> {
    let id = LearningSessionId::new(id)?;
    let kind = LearningSessionKind::parse(&kind)?;
    let subject = match kind {
        LearningSessionKind::App => {
            let platform = match app_platform {
                Some(1) => Platform::MacOs,
                Some(2) => Platform::Windows,
                _ => {
                    return Err(StorageError::CorruptData {
                        field: "learning_session.app_platform",
                    });
                }
            };
            let mut identity = AppIdentity::new(platform, target)?;
            if let Some(signer) = app_signer {
                identity = identity.with_signer_id(signer)?;
            }
            LearningSubject::App(AppLearningSubject::from_identity(&identity))
        }
        LearningSessionKind::Site => LearningSubject::Site(DomainName::normalize(&target)?),
    };
    let browser_context = browser_context.map(BrowserContextId::new).transpose()?;
    LearningSession::restore(
        id,
        subject,
        browser_context,
        LearningSessionState::parse(&state)?,
        from_sqlite_u64(started, "learning_session.started_at_unix_ms")?,
        from_sqlite_u64(expires, "learning_session.expires_at_unix_ms")?,
        stopped
            .map(|value| from_sqlite_u64(value, "learning_session.stopped_at_unix_ms"))
            .transpose()?,
    )
    .map_err(StorageError::from)
}

fn candidate_from_row(row: &Row<'_>) -> rusqlite::Result<Result<LearningCandidate, StorageError>> {
    let domain: String = row.get(0)?;
    let classification: String = row.get(1)?;
    let confidence: i64 = row.get(2)?;
    let confirmation: bool = row.get(3)?;
    let evidence: i64 = row.get(4)?;
    let first_seen: i64 = row.get(5)?;
    let last_seen: i64 = row.get(6)?;
    let main: i64 = row.get(7)?;
    let subresource: i64 = row.get(8)?;
    let redirect: i64 = row.get(9)?;
    Ok((|| {
        Ok(LearningCandidate::new(
            DomainName::normalize(&domain)?,
            LearningCandidateKind::parse(&classification).ok_or(StorageError::CorruptData {
                field: "learning_candidate.classification",
            })?,
            u16::try_from(confidence).map_err(|_| StorageError::CorruptData {
                field: "learning_candidate.confidence_millis",
            })?,
            confirmation,
            u32_from_sqlite(evidence, "learning_candidate.evidence_count")?,
            from_sqlite_u64(first_seen, "learning_candidate.first_seen_at_unix_ms")?,
            from_sqlite_u64(last_seen, "learning_candidate.last_seen_at_unix_ms")?,
            u32_from_sqlite(main, "learning_candidate.main_frame_count")?,
            u32_from_sqlite(subresource, "learning_candidate.subresource_count")?,
            u32_from_sqlite(redirect, "learning_candidate.redirect_count")?,
        ))
    })())
}

fn from_sqlite_u64(value: i64, field: &'static str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::CorruptData { field })
}
