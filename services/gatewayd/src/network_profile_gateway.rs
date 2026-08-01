use nonproxy_model::NetworkProfileId;
use nonproxy_storage::NetworkProfileReference;

use crate::{Gateway, GatewayError, clock::unix_time_ms};

impl Gateway {
    pub async fn list_network_profiles(
        &self,
    ) -> Result<(Vec<NetworkProfileReference>, u64), GatewayError> {
        self.database
            .run(|database| {
                let profiles = database.network_profiles().list()?;
                let generation = database.network_profiles().catalog_generation()?;
                Ok((profiles, generation))
            })
            .await
    }

    pub async fn save_network_profile(
        &self,
        profile: NetworkProfileReference,
        expected_revision: Option<u64>,
    ) -> Result<NetworkProfileReference, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database
                    .network_profiles()
                    .save(&profile, expected_revision, now)?;
                Ok(profile)
            })
            .await
    }

    pub async fn delete_network_profile(
        &self,
        profile_id: NetworkProfileId,
        expected_revision: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database
                    .network_profiles()
                    .delete(&profile_id, expected_revision, now)?;
                Ok(())
            })
            .await
    }
}
