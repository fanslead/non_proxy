use std::{collections::HashSet, sync::Arc};

use nonproxy_storage::{
    CredentialKind, CredentialReference, StorageError, SubscriptionRefreshCommit,
    SubscriptionSource,
};
use nonproxy_subscription::SubscriptionEndpoint;
use sha2::{Digest, Sha256};

use crate::{
    Gateway, GatewayError,
    credential_store::{
        CredentialStore, CredentialWrite, delete_credentials, load_credential, store_credentials,
    },
    subscription_fetcher::SubscriptionFetcher,
    subscription_gateway::SubscriptionState,
    subscription_prepare::prepare_subscription_refresh,
    subscription_service_types::{
        SubscriptionRefreshResult, SubscriptionServiceError, SubscriptionUpsert,
    },
};

const FAILURE_RETRY_BASE_MS: u64 = 60 * 1_000;
const FAILURE_RETRY_MAX_MS: u64 = 30 * 60 * 1_000;

#[derive(Clone)]
pub(crate) struct SubscriptionService {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    fetcher: Arc<dyn SubscriptionFetcher>,
}

impl SubscriptionService {
    pub(crate) fn new(
        gateway: Gateway,
        credential_store: Arc<dyn CredentialStore>,
        fetcher: Arc<dyn SubscriptionFetcher>,
    ) -> Self {
        Self {
            gateway,
            credential_store,
            fetcher,
        }
    }

    pub(crate) async fn upsert_at(
        &self,
        request: SubscriptionUpsert,
        now_unix_ms: u64,
    ) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
        let state = self
            .gateway
            .subscription_state(request.source_id.clone())
            .await?;
        validate_expected_revision(state.source.as_ref(), request.expected_revision)?;
        let refresh_id = random_refresh_id()?;
        let revision = state
            .source
            .as_ref()
            .map_or(Some(1), |source| source.revision().checked_add(1))
            .ok_or(SubscriptionServiceError::RevisionExhausted)?;
        let url_reference = endpoint_credential(&request.source_id, revision, &refresh_id)?;
        let source = configured_source(&request, state.source.as_ref(), url_reference, revision)?;
        let endpoint = parse_endpoint(request.endpoint_url.as_slice())?;
        let payload = self.fetcher.fetch(&endpoint).await?;
        let content_hash = hash(payload.as_slice());
        let prepared = prepare_subscription_refresh(
            &request.source_id,
            payload.as_slice(),
            &refresh_id,
            &state.ownership,
            &state.outbounds,
        )?;
        let next_refresh = next_success(now_unix_ms, request.refresh_interval_seconds);
        let old_url = state
            .source
            .as_ref()
            .map(|value| value.endpoint_credential().item_reference().to_owned());
        let expected_generation = source.content_generation();
        let node_count = prepared.nodes.len();
        let mut credentials = prepared.credentials;
        credentials.push(CredentialWrite {
            reference: source.endpoint_credential().item_reference().to_owned(),
            secret: request.endpoint_url,
        });
        let new_references = store_credentials(Arc::clone(&self.credential_store), credentials)
            .await
            .map_err(SubscriptionServiceError::CredentialWrite)?;
        let commit = self
            .gateway
            .save_subscription_refresh(
                source,
                request.expected_revision,
                expected_generation,
                content_hash,
                prepared.nodes,
                now_unix_ms,
                next_refresh,
            )
            .await;
        let commit = match commit {
            Ok(value) => value,
            Err(error) => {
                let cleanup =
                    delete_credentials(Arc::clone(&self.credential_store), new_references).await;
                return Err(SubscriptionServiceError::Commit {
                    source: error,
                    cleanup_failures: cleanup,
                });
            }
        };
        let stale = stale_credentials(old_url, &commit, &new_references);
        let cleanup_failures = delete_credentials(Arc::clone(&self.credential_store), stale).await;
        Ok(SubscriptionRefreshResult {
            source_id: request.source_id,
            revision,
            generation: commit.generation(),
            node_count,
            unchanged: false,
            cleanup_failures,
        })
    }

    pub(crate) async fn refresh_at(
        &self,
        source_id: String,
        expected_revision: u64,
        now_unix_ms: u64,
    ) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
        let state = self.gateway.subscription_state(source_id.clone()).await?;
        let source = state
            .source
            .as_ref()
            .ok_or(SubscriptionServiceError::SourceNotFound)?;
        validate_expected_revision(Some(source), Some(expected_revision))?;
        match self.refresh_loaded(&state, now_unix_ms).await {
            Ok(result) => Ok(result),
            Err(error) => self.record_failure(source, error, now_unix_ms).await,
        }
    }

    async fn refresh_loaded(
        &self,
        state: &SubscriptionState,
        now_unix_ms: u64,
    ) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
        let source = state
            .source
            .as_ref()
            .ok_or(SubscriptionServiceError::SourceNotFound)?;
        let endpoint_url = load_credential(
            Arc::clone(&self.credential_store),
            source.endpoint_credential().item_reference().to_owned(),
        )
        .await
        .map_err(|_| SubscriptionServiceError::CredentialRead)?;
        let endpoint = parse_endpoint(endpoint_url.as_slice())?;
        let payload = self.fetcher.fetch(&endpoint).await?;
        let content_hash = hash(payload.as_slice());
        let next_refresh = next_success(now_unix_ms, source.refresh_interval_seconds());
        if source.content_hash() == Some(content_hash) {
            self.gateway
                .record_subscription_unchanged(
                    source.id().to_owned(),
                    source.revision(),
                    source.content_generation(),
                    content_hash,
                    now_unix_ms,
                    next_refresh,
                )
                .await?;
            return Ok(result(source, source.content_generation(), true, 0));
        }

        let refresh_id = random_refresh_id()?;
        let prepared = prepare_subscription_refresh(
            source.id(),
            payload.as_slice(),
            &refresh_id,
            &state.ownership,
            &state.outbounds,
        )?;
        let node_count = prepared.nodes.len();
        let new_references =
            store_credentials(Arc::clone(&self.credential_store), prepared.credentials)
                .await
                .map_err(SubscriptionServiceError::CredentialWrite)?;
        let commit = self
            .gateway
            .apply_subscription_refresh(
                source.id().to_owned(),
                source.revision(),
                source.content_generation(),
                content_hash,
                prepared.nodes,
                now_unix_ms,
                next_refresh,
            )
            .await;
        let commit = match commit {
            Ok(value) => value,
            Err(error) => {
                let cleanup =
                    delete_credentials(Arc::clone(&self.credential_store), new_references).await;
                return Err(SubscriptionServiceError::Commit {
                    source: error,
                    cleanup_failures: cleanup,
                });
            }
        };
        let stale = stale_credentials(None, &commit, &new_references);
        let cleanup_failures = delete_credentials(Arc::clone(&self.credential_store), stale).await;
        Ok(SubscriptionRefreshResult {
            source_id: source.id().to_owned(),
            revision: source.revision(),
            generation: commit.generation(),
            node_count,
            unchanged: false,
            cleanup_failures,
        })
    }

    async fn record_failure(
        &self,
        source: &SubscriptionSource,
        error: SubscriptionServiceError,
        now_unix_ms: u64,
    ) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
        let next_refresh = next_failure(now_unix_ms, source.consecutive_failures());
        self.gateway
            .record_subscription_failure(
                source.id().to_owned(),
                source.revision(),
                source.content_generation(),
                error.code().to_owned(),
                now_unix_ms,
                next_refresh,
            )
            .await?;
        Err(error)
    }
}

fn configured_source(
    request: &SubscriptionUpsert,
    current: Option<&SubscriptionSource>,
    endpoint: CredentialReference,
    revision: u64,
) -> Result<SubscriptionSource, SubscriptionServiceError> {
    match current {
        Some(source) => Ok(source.reconfigured(
            &request.display_name,
            endpoint,
            request.enabled,
            request.refresh_interval_seconds,
            revision,
        )?),
        None => {
            let source = SubscriptionSource::new(
                &request.source_id,
                &request.display_name,
                endpoint,
                request.refresh_interval_seconds,
                revision,
                0,
            )?;
            Ok(if request.enabled {
                source
            } else {
                source.disabled()
            })
        }
    }
}

fn validate_expected_revision(
    current: Option<&SubscriptionSource>,
    expected: Option<u64>,
) -> Result<(), SubscriptionServiceError> {
    if matches!((current, expected), (None, None))
        || current.is_some_and(|source| Some(source.revision()) == expected)
    {
        return Ok(());
    }
    Err(GatewayError::Storage(StorageError::SubscriptionRevisionConflict).into())
}

fn endpoint_credential(
    source_id: &str,
    revision: u64,
    refresh_id: &str,
) -> Result<CredentialReference, SubscriptionServiceError> {
    Ok(CredentialReference::new(
        format!("subscription:{source_id}:url:v{revision}:{refresh_id}"),
        CredentialKind::SubscriptionUrl,
        format!("{source_id} 订阅地址"),
        revision,
    )?)
}

fn parse_endpoint(value: &[u8]) -> Result<SubscriptionEndpoint, SubscriptionServiceError> {
    let value =
        std::str::from_utf8(value).map_err(|_| SubscriptionServiceError::EndpointEncoding)?;
    Ok(SubscriptionEndpoint::parse(value)?)
}

fn hash(value: &[u8]) -> [u8; 32] {
    Sha256::digest(value).into()
}

fn random_refresh_id() -> Result<String, SubscriptionServiceError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| SubscriptionServiceError::Random(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn next_success(now: u64, interval_seconds: u32) -> u64 {
    now.saturating_add(u64::from(interval_seconds) * 1_000)
}

fn next_failure(now: u64, prior_failures: u32) -> u64 {
    let shift = prior_failures.min(5);
    let delay = FAILURE_RETRY_BASE_MS
        .saturating_mul(1_u64 << shift)
        .min(FAILURE_RETRY_MAX_MS);
    now.saturating_add(delay)
}

fn stale_credentials(
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

fn result(
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
