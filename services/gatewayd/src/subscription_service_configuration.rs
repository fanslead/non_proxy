use nonproxy_storage::{CredentialKind, CredentialReference, StorageError, SubscriptionSource};
use nonproxy_subscription::SubscriptionEndpoint;

use crate::{
    GatewayError,
    subscription_service_types::{SubscriptionServiceError, SubscriptionUpsert},
};

pub(crate) fn configured_source(
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

pub(crate) fn validate_expected_revision(
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

pub(crate) fn endpoint_credential(
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

pub(crate) fn parse_endpoint(
    value: &[u8],
) -> Result<SubscriptionEndpoint, SubscriptionServiceError> {
    let value =
        std::str::from_utf8(value).map_err(|_| SubscriptionServiceError::EndpointEncoding)?;
    Ok(SubscriptionEndpoint::parse(value)?)
}
