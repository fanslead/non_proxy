use std::collections::HashSet;

use nonproxy_storage::{SubscriptionRefreshCommit, SubscriptionSource};
use sha2::{Digest, Sha256};

use crate::subscription_service_types::{SubscriptionRefreshResult, SubscriptionServiceError};

const FAILURE_RETRY_BASE_MS: u64 = 60 * 1_000;
const FAILURE_RETRY_MAX_MS: u64 = 30 * 60 * 1_000;

pub(crate) fn content_hash(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

pub(crate) fn random_refresh_id() -> Result<String, SubscriptionServiceError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| SubscriptionServiceError::Random(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn next_success(now: u64, interval_seconds: u32) -> u64 {
    now.saturating_add(u64::from(interval_seconds) * 1_000)
}

pub(crate) fn next_failure(now: u64, prior_failures: u32) -> u64 {
    let shift = prior_failures.min(5);
    let delay = FAILURE_RETRY_BASE_MS
        .saturating_mul(1_u64 << shift)
        .min(FAILURE_RETRY_MAX_MS);
    now.saturating_add(delay)
}

pub(crate) fn stale_credentials(
    old_url: Option<String>,
    commit: &SubscriptionRefreshCommit,
    new_references: &HashSet<String>,
) -> HashSet<String> {
    old_url
        .into_iter()
        .chain(commit.replaced_credential_references().iter().cloned())
        .filter(|reference| !new_references.contains(reference))
        .collect()
}

pub(crate) fn refresh_result(
    source: &SubscriptionSource,
    generation: u64,
    unchanged: bool,
    cleanup_failures: usize,
) -> SubscriptionRefreshResult {
    SubscriptionRefreshResult {
        source_id: source.id().to_owned(),
        revision: source.revision(),
        generation,
        node_count: source.node_count() as usize,
        unchanged,
        cleanup_failures,
    }
}
