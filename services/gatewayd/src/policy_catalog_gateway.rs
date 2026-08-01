use nonproxy_model::{Policy, PolicyId};

use crate::{
    Gateway, GatewayError,
    clock::unix_time_ms,
    runtime_policy::{RuntimePolicyCatalog, RuntimePolicyRecord, build_runtime_catalog},
};

impl Gateway {
    pub async fn list_policies(&self) -> Result<Vec<Policy>, GatewayError> {
        self.database
            .run(|database| Ok(database.policies().list()?))
            .await
    }

    pub async fn list_runtime_policies(&self) -> Result<Vec<RuntimePolicyRecord>, GatewayError> {
        Ok(self.runtime_policy_catalog().await?.records().to_vec())
    }

    pub async fn runtime_policy_catalog(&self) -> Result<RuntimePolicyCatalog, GatewayError> {
        self.database
            .run(|database| {
                let generation = database.policies().catalog_generation()?;
                let current = database.policies().list()?;
                let active = database.snapshots().active()?;
                let pending = database.snapshots().pending()?;
                let previous_effective = match active.as_ref() {
                    Some(record) => database
                        .snapshots()
                        .previous_effective_version(record.artifact().snapshot_version())?,
                    None => None,
                };
                build_runtime_catalog(
                    generation,
                    current,
                    active.as_ref(),
                    pending.as_ref(),
                    previous_effective,
                )
            })
            .await
    }

    pub async fn save_policy(
        &self,
        policy: Policy,
        expected_revision: Option<u64>,
    ) -> Result<Policy, GatewayError> {
        crate::system_policies::validate_user_mutation(&policy)?;
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database.policies().save(&policy, expected_revision, now)?;
                Ok(policy)
            })
            .await
    }

    pub async fn delete_policy(
        &self,
        policy_id: PolicyId,
        expected_revision: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database
                    .policies()
                    .delete(&policy_id, expected_revision, now)?;
                Ok(())
            })
            .await
    }
}
