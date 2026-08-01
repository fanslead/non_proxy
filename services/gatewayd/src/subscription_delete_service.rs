use std::sync::Arc;

use crate::{
    Gateway,
    credential_cleanup_service::cleanup_queued_references,
    credential_store::CredentialStore,
    subscription_service_types::{SubscriptionDeleteResult, SubscriptionServiceError},
};

pub(crate) async fn delete_subscription(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    source_id: String,
    expected_revision: u64,
    now_unix_ms: u64,
) -> Result<SubscriptionDeleteResult, SubscriptionServiceError> {
    let commit = gateway
        .delete_subscription_source(source_id.clone(), expected_revision, now_unix_ms)
        .await?;
    let entries = commit
        .credential_references()
        .iter()
        .cloned()
        .map(|reference| (reference, 0))
        .collect();
    let cleanup_failures =
        cleanup_queued_references(gateway, credential_store, entries, now_unix_ms).await;
    Ok(SubscriptionDeleteResult {
        source_id,
        outbound_count: commit.outbound_count(),
        cleanup_failures,
    })
}
