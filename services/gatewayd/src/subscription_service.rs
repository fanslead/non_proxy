use std::sync::Arc;

use nonproxy_storage::SubscriptionSource;

use crate::{
    Gateway,
    credential_cleanup_service::{
        cleanup_queued_references, queue_and_cleanup_references, retry_due_cleanup,
        store_credentials_with_cleanup,
    },
    credential_store::{CredentialStore, CredentialWrite, load_credential},
    subscription_delete_service::delete_subscription,
    subscription_fetcher::SubscriptionFetcher,
    subscription_gateway::SubscriptionState,
    subscription_prepare::prepare_subscription_refresh,
    subscription_service_configuration::{
        configured_source, endpoint_credential, parse_endpoint, validate_expected_revision,
    },
    subscription_service_helpers::{
        content_hash, next_failure, next_success, random_refresh_id, refresh_result,
        stale_credentials,
    },
    subscription_service_types::{
        SubscriptionDeleteResult, SubscriptionRefreshResult, SubscriptionServiceError,
        SubscriptionUpsert,
    },
    subscription_settings_service::update_subscription_settings,
    subscription_task_tracker::SubscriptionTaskTracker,
};

#[derive(Clone)]
pub(crate) struct SubscriptionService {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    fetcher: Arc<dyn SubscriptionFetcher>,
    tasks: SubscriptionTaskTracker,
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
            tasks: SubscriptionTaskTracker::new(),
        }
    }

    pub(crate) async fn upsert_at(
        &self,
        request: SubscriptionUpsert,
        now_unix_ms: u64,
    ) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
        let task = self
            .tasks
            .start()
            .ok_or(SubscriptionServiceError::TaskClosed)?;
        let service = self.clone();
        tokio::spawn(async move {
            let _task = task;
            service.upsert_inner(request, now_unix_ms).await
        })
        .await
        .map_err(|_| SubscriptionServiceError::TaskFailed)?
    }

    async fn upsert_inner(
        &self,
        mut request: SubscriptionUpsert,
        now_unix_ms: u64,
    ) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
        let state = self
            .gateway
            .subscription_state(request.source_id.clone())
            .await?;
        validate_expected_revision(state.source.as_ref(), request.expected_revision)?;
        let revision = state
            .source
            .as_ref()
            .map_or(Some(1), |source| source.revision().checked_add(1))
            .ok_or(SubscriptionServiceError::RevisionExhausted)?;
        let Some(endpoint_url) = request.endpoint_url.take() else {
            return update_subscription_settings(
                &self.gateway,
                &request,
                state.source.as_ref(),
                revision,
                now_unix_ms,
            )
            .await;
        };
        let refresh_id = random_refresh_id()?;
        let url_reference = endpoint_credential(&request.source_id, revision, &refresh_id)?;
        let source = configured_source(&request, state.source.as_ref(), url_reference, revision)?;
        let endpoint = parse_endpoint(endpoint_url.as_slice())?;
        let payload = self.fetcher.fetch(&endpoint).await?;
        let content_hash = content_hash(payload.as_slice());
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
            secret: endpoint_url,
        });
        let new_references = store_credentials_with_cleanup(
            &self.gateway,
            Arc::clone(&self.credential_store),
            credentials,
            now_unix_ms,
        )
        .await
        .map_err(SubscriptionServiceError::from)?;
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
                let cleanup = queue_and_cleanup_references(
                    &self.gateway,
                    Arc::clone(&self.credential_store),
                    new_references,
                    now_unix_ms,
                )
                .await;
                return Err(SubscriptionServiceError::Commit {
                    source: error,
                    cleanup_failures: cleanup.failure_count(),
                    cleanup_retry_persisted: cleanup.retry_persisted(),
                });
            }
        };
        let stale = stale_credentials(old_url, &commit, &new_references);
        let cleanup_failures = cleanup_queued_references(
            &self.gateway,
            Arc::clone(&self.credential_store),
            stale.into_iter().map(|reference| (reference, 0)).collect(),
            now_unix_ms,
        )
        .await;
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
        let task = self
            .tasks
            .start()
            .ok_or(SubscriptionServiceError::TaskClosed)?;
        let service = self.clone();
        tokio::spawn(async move {
            let _task = task;
            service
                .refresh_inner(source_id, expected_revision, now_unix_ms)
                .await
        })
        .await
        .map_err(|_| SubscriptionServiceError::TaskFailed)?
    }

    pub(crate) async fn delete_at(
        &self,
        source_id: String,
        expected_revision: u64,
        now_unix_ms: u64,
    ) -> Result<SubscriptionDeleteResult, SubscriptionServiceError> {
        let task = self
            .tasks
            .start()
            .ok_or(SubscriptionServiceError::TaskClosed)?;
        let gateway = self.gateway.clone();
        let credential_store = Arc::clone(&self.credential_store);
        tokio::spawn(async move {
            let _task = task;
            delete_subscription(
                &gateway,
                credential_store,
                source_id,
                expected_revision,
                now_unix_ms,
            )
            .await
        })
        .await
        .map_err(|_| SubscriptionServiceError::TaskFailed)?
    }

    pub(crate) async fn retry_credential_cleanup_at(
        &self,
        now_unix_ms: u64,
    ) -> Result<usize, SubscriptionServiceError> {
        let task = self
            .tasks
            .start()
            .ok_or(SubscriptionServiceError::TaskClosed)?;
        let service = self.clone();
        tokio::spawn(async move {
            let _task = task;
            retry_due_cleanup(
                &service.gateway,
                Arc::clone(&service.credential_store),
                now_unix_ms,
            )
            .await
            .map_err(SubscriptionServiceError::Gateway)
        })
        .await
        .map_err(|_| SubscriptionServiceError::TaskFailed)?
    }

    pub(crate) fn close(&self) {
        self.tasks.close();
    }

    pub(crate) async fn wait_idle(&self) {
        self.tasks.wait_idle().await;
    }

    async fn refresh_inner(
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
        let content_hash = content_hash(payload.as_slice());
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
            return Ok(refresh_result(source, source.content_generation(), true, 0));
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
        let new_references = store_credentials_with_cleanup(
            &self.gateway,
            Arc::clone(&self.credential_store),
            prepared.credentials,
            now_unix_ms,
        )
        .await
        .map_err(SubscriptionServiceError::from)?;
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
                let cleanup = queue_and_cleanup_references(
                    &self.gateway,
                    Arc::clone(&self.credential_store),
                    new_references,
                    now_unix_ms,
                )
                .await;
                return Err(SubscriptionServiceError::Commit {
                    source: error,
                    cleanup_failures: cleanup.failure_count(),
                    cleanup_retry_persisted: cleanup.retry_persisted(),
                });
            }
        };
        let stale = stale_credentials(None, &commit, &new_references);
        let cleanup_failures = cleanup_queued_references(
            &self.gateway,
            Arc::clone(&self.credential_store),
            stale.into_iter().map(|reference| (reference, 0)).collect(),
            now_unix_ms,
        )
        .await;
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
