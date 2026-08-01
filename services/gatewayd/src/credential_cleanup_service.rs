use std::{collections::HashSet, sync::Arc};

use nonproxy_storage::CredentialCleanupEntry;

use crate::{
    Gateway, GatewayError,
    credential_store::{CredentialStore, CredentialWrite, delete_credentials, store_credentials},
};

const RETRY_BASE_MS: u64 = 60 * 1_000;
const RETRY_MAX_MS: u64 = 30 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CredentialCleanupOutcome {
    failure_count: usize,
    retry_persisted: bool,
}

impl CredentialCleanupOutcome {
    #[must_use]
    pub(crate) const fn failure_count(self) -> usize {
        self.failure_count
    }

    #[must_use]
    pub(crate) const fn retry_persisted(self) -> bool {
        self.retry_persisted
    }
}

pub(crate) async fn store_credentials_with_cleanup(
    gateway: &Gateway,
    store: Arc<dyn CredentialStore>,
    credentials: Vec<CredentialWrite>,
    now_unix_ms: u64,
) -> Result<HashSet<String>, CredentialCleanupOutcome> {
    match store_credentials(Arc::clone(&store), credentials).await {
        Ok(references) => Ok(references),
        Err(failure) => Err(queue_and_cleanup_references(
            gateway,
            store,
            failure.into_failed_references(),
            now_unix_ms,
        )
        .await),
    }
}

pub(crate) async fn cleanup_queued_references(
    gateway: &Gateway,
    store: Arc<dyn CredentialStore>,
    entries: Vec<(String, u32)>,
    now_unix_ms: u64,
) -> usize {
    let attempts = entries
        .iter()
        .cloned()
        .collect::<std::collections::HashMap<_, _>>();
    let references = attempts.keys().cloned().collect::<HashSet<_>>();
    let result = delete_credentials(store, references).await;
    let failure_count = result.failure_count();
    let (succeeded, failed) = result.into_parts();
    if !succeeded.is_empty() {
        let _result = gateway
            .complete_credential_cleanup(succeeded.into_iter().collect())
            .await;
    }
    if !failed.is_empty() {
        let failures = failed
            .into_iter()
            .map(|reference| {
                let prior_attempts = match attempts.get(&reference) {
                    Some(value) => *value,
                    None => 0,
                };
                (reference, next_attempt(now_unix_ms, prior_attempts))
            })
            .collect();
        let _result = gateway
            .record_credential_cleanup_failures(failures, now_unix_ms)
            .await;
    }
    failure_count
}

pub(crate) async fn queue_and_cleanup_references(
    gateway: &Gateway,
    store: Arc<dyn CredentialStore>,
    references: HashSet<String>,
    now_unix_ms: u64,
) -> CredentialCleanupOutcome {
    let retry_references = references.iter().cloned().collect::<Vec<_>>();
    let entries = references
        .iter()
        .cloned()
        .map(|reference| (reference, 0))
        .collect::<Vec<_>>();
    let queued = gateway
        .enqueue_credential_cleanup(references.into_iter().collect(), now_unix_ms)
        .await
        .is_ok();
    let failure_count = cleanup_queued_references(gateway, store, entries, now_unix_ms).await;
    let retry_persisted = if queued || failure_count == 0 {
        true
    } else {
        gateway
            .enqueue_credential_cleanup(retry_references, now_unix_ms)
            .await
            .is_ok()
    };
    CredentialCleanupOutcome {
        failure_count,
        retry_persisted,
    }
}

pub(crate) async fn retry_due_cleanup(
    gateway: &Gateway,
    store: Arc<dyn CredentialStore>,
    now_unix_ms: u64,
) -> Result<usize, GatewayError> {
    let entries = gateway.due_credential_cleanup(now_unix_ms, 100).await?;
    let entries = entries.into_iter().map(entry_parts).collect::<Vec<_>>();
    Ok(cleanup_queued_references(gateway, store, entries, now_unix_ms).await)
}

fn entry_parts(entry: CredentialCleanupEntry) -> (String, u32) {
    (entry.reference().to_owned(), entry.attempts())
}

fn next_attempt(now_unix_ms: u64, prior_attempts: u32) -> u64 {
    let delay = RETRY_BASE_MS
        .saturating_mul(1_u64 << prior_attempts.min(5))
        .min(RETRY_MAX_MS);
    now_unix_ms.saturating_add(delay)
}
