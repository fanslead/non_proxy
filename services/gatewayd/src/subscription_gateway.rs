use nonproxy_storage::{
    OutboundReference, SubscriptionNode, SubscriptionNodeOwnership, SubscriptionRefreshCommit,
    SubscriptionSource,
};

use crate::{Gateway, GatewayError};

pub(crate) struct SubscriptionState {
    pub(crate) source: Option<SubscriptionSource>,
    pub(crate) ownership: Vec<SubscriptionNodeOwnership>,
    pub(crate) outbounds: Vec<OutboundReference>,
}

impl Gateway {
    pub(crate) async fn list_subscription_sources(
        &self,
    ) -> Result<Vec<SubscriptionSource>, GatewayError> {
        self.database
            .run(|database| Ok(database.subscriptions().list()?))
            .await
    }

    pub(crate) async fn subscription_source(
        &self,
        source_id: String,
    ) -> Result<Option<SubscriptionSource>, GatewayError> {
        self.database
            .run(move |database| Ok(database.subscriptions().get(&source_id)?))
            .await
    }

    pub(crate) async fn subscription_state(
        &self,
        source_id: String,
    ) -> Result<SubscriptionState, GatewayError> {
        self.database
            .run(move |database| {
                Ok(SubscriptionState {
                    source: database.subscriptions().get(&source_id)?,
                    ownership: database.subscriptions().ownership(&source_id)?,
                    outbounds: database.outbounds().list()?,
                })
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn save_subscription_refresh(
        &self,
        source: SubscriptionSource,
        expected_revision: Option<u64>,
        expected_generation: u64,
        content_hash: [u8; 32],
        nodes: Vec<SubscriptionNode>,
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<SubscriptionRefreshCommit, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                Ok(database.subscriptions().save_and_apply_refresh(
                    &source,
                    expected_revision,
                    expected_generation,
                    content_hash,
                    &nodes,
                    attempted_at_unix_ms,
                    next_refresh_at_unix_ms,
                )?)
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn apply_subscription_refresh(
        &self,
        source_id: String,
        expected_revision: u64,
        expected_generation: u64,
        content_hash: [u8; 32],
        nodes: Vec<SubscriptionNode>,
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<SubscriptionRefreshCommit, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                Ok(database.subscriptions().apply_refresh(
                    &source_id,
                    expected_revision,
                    expected_generation,
                    content_hash,
                    &nodes,
                    attempted_at_unix_ms,
                    next_refresh_at_unix_ms,
                )?)
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_subscription_unchanged(
        &self,
        source_id: String,
        expected_revision: u64,
        expected_generation: u64,
        content_hash: [u8; 32],
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                database.subscriptions().record_unchanged(
                    &source_id,
                    expected_revision,
                    expected_generation,
                    content_hash,
                    attempted_at_unix_ms,
                    next_refresh_at_unix_ms,
                )?;
                Ok(())
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_subscription_failure(
        &self,
        source_id: String,
        expected_revision: u64,
        expected_generation: u64,
        error_code: String,
        attempted_at_unix_ms: u64,
        next_refresh_at_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                database.subscriptions().record_failure(
                    &source_id,
                    expected_revision,
                    expected_generation,
                    &error_code,
                    attempted_at_unix_ms,
                    next_refresh_at_unix_ms,
                )?;
                Ok(())
            })
            .await
    }
}
