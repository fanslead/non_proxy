use nonproxy_storage::SubscriptionSource;

use crate::{
    Gateway,
    subscription_service_helpers::refresh_result,
    subscription_service_types::{
        SubscriptionRefreshResult, SubscriptionServiceError, SubscriptionUpsert,
    },
};

pub(crate) async fn update_subscription_settings(
    gateway: &Gateway,
    request: &SubscriptionUpsert,
    current: Option<&SubscriptionSource>,
    revision: u64,
    now_unix_ms: u64,
) -> Result<SubscriptionRefreshResult, SubscriptionServiceError> {
    let current = current.ok_or(SubscriptionServiceError::EndpointRequired)?;
    let source = current.settings_updated(
        &request.display_name,
        request.enabled,
        request.refresh_interval_seconds,
        revision,
        now_unix_ms,
    )?;
    let result = refresh_result(&source, source.content_generation(), true, 0);
    gateway
        .save_subscription_source(source, current.revision(), now_unix_ms)
        .await?;
    Ok(result)
}
