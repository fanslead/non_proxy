use std::{path::Path, sync::Arc};

use nonproxy_model::{DecisionSpec, Policy, PolicyId};
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{OutboundReference, PolicyDatabase, SnapshotArtifact, SnapshotRecord};
use tokio::sync::Mutex;

use crate::{
    GatewayError,
    clock::unix_time_ms,
    database_executor::DatabaseExecutor,
    event_hub::EventHub,
    runtime_policy::{RuntimePolicyCatalog, RuntimePolicyRecord, build_runtime_catalog},
    snapshot_payload,
};

#[derive(Clone)]
pub struct Gateway {
    database: DatabaseExecutor,
    capabilities: CompileCapabilities,
    mutation_gate: Arc<Mutex<()>>,
    events: EventHub,
}

#[derive(Clone, Debug)]
pub struct PublishedSnapshot {
    artifact: SnapshotArtifact,
    default_decision: DecisionSpec,
}

#[derive(Clone, Debug)]
pub struct GatewayStatus {
    pub active: Option<SnapshotRecord>,
    pub pending: Option<SnapshotRecord>,
    pub policy_count: usize,
}

impl Gateway {
    pub async fn open(
        database_path: impl AsRef<Path>,
        capabilities: CompileCapabilities,
    ) -> Result<Self, GatewayError> {
        let path = database_path.as_ref().to_path_buf();
        let now = unix_time_ms()?;
        let database = tokio::task::spawn_blocking(move || PolicyDatabase::open(path, now))
            .await
            .map_err(|error| GatewayError::DatabaseTask(error.to_string()))??;
        Ok(Self::new(database, capabilities))
    }

    #[must_use]
    pub fn new(database: PolicyDatabase, capabilities: CompileCapabilities) -> Self {
        Self {
            database: DatabaseExecutor::new(database),
            capabilities,
            mutation_gate: Arc::new(Mutex::new(())),
            events: EventHub::new(),
        }
    }

    #[must_use]
    pub const fn capabilities(&self) -> &CompileCapabilities {
        &self.capabilities
    }

    #[must_use]
    pub const fn events(&self) -> &EventHub {
        &self.events
    }

    pub async fn status(&self) -> Result<GatewayStatus, GatewayError> {
        self.database
            .run(|database| {
                let active = database.snapshots().active()?;
                let pending = database.snapshots().pending()?;
                let policy_count = database.policies().list()?.len();
                Ok(GatewayStatus {
                    active,
                    pending,
                    policy_count,
                })
            })
            .await
    }

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
                build_runtime_catalog(generation, current, active.as_ref(), pending.as_ref())
            })
            .await
    }

    pub async fn save_policy(
        &self,
        policy: Policy,
        expected_revision: Option<u64>,
    ) -> Result<Policy, GatewayError> {
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

    pub async fn compile_and_stage(&self) -> Result<PublishedSnapshot, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let capabilities = self.capabilities.clone();
        self.database
            .run(move |database| {
                let policies = database.policies().list()?;
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let snapshot_version = current
                    .checked_add(1)
                    .ok_or(GatewayError::SnapshotVersionExhausted)?;
                let default_decision = DecisionSpec::direct();
                let compiled = PolicyCompiler::compile(CompileRequest::new(
                    snapshot_version,
                    now,
                    default_decision.clone(),
                    policies.clone(),
                    capabilities.clone(),
                ))?;
                let payload =
                    snapshot_payload::encode(&policies, &capabilities, &default_decision)?;
                let metadata = compiled.metadata();
                let artifact = SnapshotArtifact::new(
                    metadata.snapshot_version(),
                    metadata.schema_version(),
                    metadata.created_at_unix_ms(),
                    *metadata.content_hash(),
                    metadata.policy_count(),
                    payload,
                )?;
                database.snapshots().stage(&artifact)?;
                Ok(PublishedSnapshot {
                    artifact,
                    default_decision,
                })
            })
            .await
    }

    pub async fn stage_rollback(
        &self,
        target_snapshot_version: u64,
    ) -> Result<PublishedSnapshot, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let next = current
                    .checked_add(1)
                    .ok_or(GatewayError::SnapshotVersionExhausted)?;
                let artifact =
                    database
                        .snapshots()
                        .stage_rollback(next, target_snapshot_version, now)?;
                Ok(PublishedSnapshot {
                    artifact,
                    default_decision: DecisionSpec::direct(),
                })
            })
            .await
    }

    pub async fn list_outbounds(&self) -> Result<Vec<OutboundReference>, GatewayError> {
        self.database
            .run(|database| Ok(database.outbounds().list()?))
            .await
    }
}

impl PublishedSnapshot {
    #[must_use]
    pub const fn artifact(&self) -> &SnapshotArtifact {
        &self.artifact
    }

    #[must_use]
    pub const fn default_decision(&self) -> &DecisionSpec {
        &self.default_decision
    }
}
