use nonproxy_model::OutboundId;
use nonproxy_storage::OutboundReference;

use crate::{Gateway, GatewayError, clock::unix_time_ms};

impl Gateway {
    pub async fn list_outbounds(&self) -> Result<Vec<OutboundReference>, GatewayError> {
        self.database
            .run(|database| Ok(database.outbounds().list()?))
            .await
    }

    pub async fn outbound(
        &self,
        outbound_id: OutboundId,
    ) -> Result<Option<OutboundReference>, GatewayError> {
        self.database
            .run(move |database| Ok(database.outbounds().get(&outbound_id)?))
            .await
    }

    pub async fn save_outbounds(
        &self,
        outbounds: Vec<(OutboundReference, Option<u64>)>,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database.outbounds().save_batch(&outbounds, now)?;
                Ok(())
            })
            .await
    }
}
