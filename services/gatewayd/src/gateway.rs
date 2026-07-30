use std::{path::Path, sync::Arc};

use nonproxy_model::{DecisionSpec, OutboundId, Policy, PolicyId};
use nonproxy_policy::OutboundCapabilities;
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{
    OutboundReference, PolicyDatabase, ProviderAck, ProviderAckState, SnapshotArtifact,
    SnapshotRecord, StorageError,
};
use tokio::sync::Mutex;

use crate::{
    GatewayError,
    clock::unix_time_ms,
    database_executor::DatabaseExecutor,
    event_hub::EventHub,
    provider_health::ProviderHealthRegistry,
    provider_requirements,
    runtime_policy::{RuntimePolicyCatalog, RuntimePolicyRecord, build_runtime_catalog},
    snapshot_payload,
};

#[derive(Clone)]
pub struct Gateway {
    database: DatabaseExecutor,
    capabilities: CompileCapabilities,
    mutation_gate: Arc<Mutex<()>>,
    events: EventHub,
    provider_health: ProviderHealthRegistry,
}

#[derive(Clone, Debug)]
pub struct PublishedSnapshot {
    artifact: SnapshotArtifact,
    default_decision: DecisionSpec,
}

#[derive(Clone, Debug)]
pub struct ProviderSnapshot {
    record: SnapshotRecord,
    default_decision: DecisionSpec,
}

#[derive(Clone, Debug)]
pub struct GatewayStatus {
    pub active: Option<SnapshotRecord>,
    pub pending: Option<SnapshotRecord>,
    pub policy_count: usize,
    pub data_plane_ready: bool,
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
            provider_health: ProviderHealthRegistry::new(),
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
        let mut status = self
            .database
            .run(|database| {
                let active = database.snapshots().active()?;
                let pending = database.snapshots().pending()?;
                let policy_count = database.policies().list()?.len();
                Ok(GatewayStatus {
                    active,
                    pending,
                    policy_count,
                    data_plane_ready: false,
                })
            })
            .await?;
        if let Some(active) = status.active.as_ref() {
            status.data_plane_ready = self.provider_health.all_ready(
                provider_requirements::required_provider_ids(),
                active.artifact().snapshot_version(),
                unix_time_ms()?,
            )?;
        }
        Ok(status)
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
                let outbounds = database.outbounds().list()?;
                let capabilities = capabilities_for_outbounds(capabilities, &outbounds);
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

    pub async fn next_provider_generation(&self, provider_id: String) -> Result<u64, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        self.database
            .run(move |database| Ok(database.providers().next_generation(&provider_id)?))
            .await
    }

    pub fn mark_provider_registered(
        &self,
        provider_id: &str,
        generation: u64,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        self.provider_health
            .registered(provider_id, generation, now_unix_ms)
    }

    pub fn report_provider_health(
        &self,
        provider_id: &str,
        generation: u64,
        state: nonproxy_proto::events::v1::RuntimeState,
        active_snapshot_version: u64,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        self.provider_health.update(
            provider_id,
            generation,
            state,
            active_snapshot_version,
            now_unix_ms,
        )
    }

    pub async fn provider_snapshot(
        &self,
        known_snapshot_version: u64,
    ) -> Result<Option<ProviderSnapshot>, GatewayError> {
        self.database
            .run(move |database| {
                let candidate = database
                    .snapshots()
                    .pending()?
                    .or(database.snapshots().active()?);
                let Some(record) = candidate else {
                    return Ok(None);
                };
                if record.status() == nonproxy_storage::SnapshotStatus::Active
                    && record.artifact().snapshot_version() == known_snapshot_version
                {
                    return Ok(None);
                }
                let (_policies, _capabilities, default_decision) =
                    snapshot_payload::decode(record.artifact().payload())?;
                Ok(Some(ProviderSnapshot {
                    record,
                    default_decision,
                }))
            })
            .await
    }

    pub async fn acknowledge_provider_snapshot(
        &self,
        snapshot_version: u64,
        acknowledgement: ProviderAck,
        required_provider_ids: Vec<String>,
    ) -> Result<SnapshotRecord, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                database
                    .snapshots()
                    .record_ack(snapshot_version, &acknowledgement)?;
                if acknowledgement.state() == ProviderAckState::Loaded {
                    match database.snapshots().activate(
                        snapshot_version,
                        &required_provider_ids,
                        now,
                    ) {
                        Ok(()) | Err(StorageError::ProviderAcknowledgementMissing) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                database
                    .snapshots()
                    .get(snapshot_version)?
                    .ok_or_else(|| StorageError::SnapshotNotFound.into())
            })
            .await
    }
}

fn capabilities_for_outbounds(
    mut capabilities: CompileCapabilities,
    outbounds: &[OutboundReference],
) -> CompileCapabilities {
    for outbound in outbounds.iter().filter(|value| value.enabled()) {
        let outbound_capabilities = match outbound.kind() {
            nonproxy_storage::OutboundKind::HttpConnect => {
                OutboundCapabilities::new(true, false, true, true)
            }
            nonproxy_storage::OutboundKind::Socks5 => OutboundCapabilities::full(),
            nonproxy_storage::OutboundKind::Adapter => continue,
        };
        capabilities = capabilities.with_outbound(outbound.id().clone(), outbound_capabilities);
    }
    capabilities
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

impl ProviderSnapshot {
    #[must_use]
    pub const fn record(&self) -> &SnapshotRecord {
        &self.record
    }

    #[must_use]
    pub const fn default_decision(&self) -> &DecisionSpec {
        &self.default_decision
    }
}
