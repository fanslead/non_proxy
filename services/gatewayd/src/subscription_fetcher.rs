use nonproxy_subscription::{SubscriptionClient, SubscriptionEndpoint, SubscriptionFetchError};
use zeroize::Zeroizing;

#[tonic::async_trait]
pub(crate) trait SubscriptionFetcher: Send + Sync {
    async fn fetch(
        &self,
        endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError>;
}

#[tonic::async_trait]
impl SubscriptionFetcher for SubscriptionClient {
    async fn fetch(
        &self,
        endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError> {
        SubscriptionClient::fetch(self, endpoint).await
    }
}
