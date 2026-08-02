use nonproxy_model::OutboundGroupId;
use nonproxy_storage::OutboundGroup;

use crate::{Gateway, GatewayError, clock::unix_time_ms};

impl Gateway {
    pub async fn list_outbound_groups(&self) -> Result<Vec<OutboundGroup>, GatewayError> {
        self.database
            .run(|database| Ok(database.outbound_groups().list()?))
            .await
    }

    pub async fn save_outbound_group(
        &self,
        group: OutboundGroup,
        expected_revision: Option<u64>,
    ) -> Result<OutboundGroup, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database
                    .outbound_groups()
                    .save(&group, expected_revision, now)?;
                Ok(group)
            })
            .await
    }

    pub async fn delete_outbound_group(
        &self,
        group_id: OutboundGroupId,
        expected_revision: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let deleted_group_id = group_id.clone();
        self.database
            .run(move |database| {
                database
                    .outbound_groups()
                    .delete(&deleted_group_id, expected_revision, now)?;
                Ok(())
            })
            .await?;
        self.outbound_group_selections.forget(&group_id).await;
        Ok(())
    }
}
