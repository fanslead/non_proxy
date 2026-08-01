use nonproxy_storage::{
    CredentialCleanupEntry, OutboundReference, SubscriptionDeleteCommit, SubscriptionNode,
    SubscriptionNodeOwnership, SubscriptionRefreshCommit, SubscriptionSource,
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

    pub(crate) async fn due_subscription_sources(
        &self,
        now_unix_ms: u64,
        limit: u32,
    ) -> Result<Vec<SubscriptionSource>, GatewayError> {
        self.database
            .run(move |database| Ok(database.subscriptions().due(now_unix_ms, limit)?))
            .await
    }

    pub(crate) async fn delete_subscription_source(
        &self,
        source_id: String,
        expected_revision: u64,
        deleted_at_unix_ms: u64,
    ) -> Result<SubscriptionDeleteCommit, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                Ok(database.subscriptions().delete(
                    &source_id,
                    expected_revision,
                    deleted_at_unix_ms,
                )?)
            })
            .await
    }

    pub(crate) async fn due_credential_cleanup(
        &self,
        now_unix_ms: u64,
        limit: u32,
    ) -> Result<Vec<CredentialCleanupEntry>, GatewayError> {
        self.database
            .run(move |database| Ok(database.credential_cleanup().due(now_unix_ms, limit)?))
            .await
    }

    pub(crate) async fn enqueue_credential_cleanup(
        &self,
        references: Vec<String>,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                database
                    .credential_cleanup()
                    .enqueue(references, now_unix_ms)?;
                Ok(())
            })
            .await
    }

    pub(crate) async fn complete_credential_cleanup(
        &self,
        references: Vec<String>,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                database.credential_cleanup().complete(&references)?;
                Ok(())
            })
            .await
    }

    pub(crate) async fn record_credential_cleanup_failures(
        &self,
        failures: Vec<(String, u64)>,
        attempted_at_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| {
                database.credential_cleanup().record_failures(
                    &failures,
                    "NP_CREDENTIAL_STORE_FAILED",
                    attempted_at_unix_ms,
                )?;
                Ok(())
            })
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
