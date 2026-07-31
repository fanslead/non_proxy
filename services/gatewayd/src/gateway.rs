use std::{
    net::{IpAddr, Ipv6Addr},
    path::Path,
    sync::Arc,
};

use nonproxy_dns::{SyntheticAddressFamily, SyntheticAddressSpace};
use nonproxy_model::{DecisionSpec, DomainName, OutboundId, Policy, PolicyId};
use nonproxy_policy::CompiledPolicySnapshot;
use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
use nonproxy_storage::{
    OutboundReference, PolicyDatabase, ProviderAck, ProviderAckState, RoutingSettings,
    SnapshotArtifact, SnapshotRecord, StorageError, SyntheticDnsBinding,
};
use tokio::sync::Mutex;

use crate::{
    GatewayError,
    clock::unix_time_ms,
    database_executor::DatabaseExecutor,
    decision_snapshot_cache::DecisionSnapshotCache,
    event_hub::EventHub,
    outbound_health::{OutboundHealthObservation, OutboundHealthRegistry},
    provider_health::ProviderHealthRegistry,
    provider_requirements,
    routing_gateway::decision_for_route,
    runtime_policy::{RuntimePolicyCatalog, RuntimePolicyRecord, build_runtime_catalog},
    snapshot_builder::build_snapshot,
    snapshot_payload,
};

#[derive(Clone)]
pub struct Gateway {
    pub(crate) database: DatabaseExecutor,
    capabilities: CompileCapabilities,
    pub(crate) mutation_gate: Arc<Mutex<()>>,
    events: EventHub,
    outbound_health: OutboundHealthRegistry,
    provider_health: ProviderHealthRegistry,
    pub(crate) decision_snapshots: DecisionSnapshotCache,
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
    pub routing: RoutingSettings,
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
            outbound_health: OutboundHealthRegistry::new(),
            provider_health: ProviderHealthRegistry::new(),
            decision_snapshots: DecisionSnapshotCache::default(),
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
                let routing = database.routing_settings().get()?;
                Ok(GatewayStatus {
                    active,
                    pending,
                    policy_count,
                    data_plane_ready: false,
                    routing,
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
        self.compile_and_stage_locked(now).await
    }

    pub(crate) async fn compile_and_stage_locked(
        &self,
        now_unix_ms: u64,
    ) -> Result<PublishedSnapshot, GatewayError> {
        let capabilities = self.capabilities.clone();
        self.database
            .run(move |database| {
                let policies = database.policies().list()?;
                let outbounds = database.outbounds().list()?;
                let routing = database.routing_settings().get()?;
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let snapshot_version = current
                    .checked_add(1)
                    .ok_or(GatewayError::SnapshotVersionExhausted)?;
                let published = build_snapshot(
                    capabilities,
                    &policies,
                    &outbounds,
                    decision_for_route(routing.route())?,
                    snapshot_version,
                    now_unix_ms,
                )?;
                database.snapshots().stage(published.artifact())?;
                Ok(published)
            })
            .await
    }

    pub async fn stage_rollback(
        &self,
        target_snapshot_version: u64,
    ) -> Result<PublishedSnapshot, GatewayError> {
        self.stage_rollback_with_route(target_snapshot_version)
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

    pub fn report_outbound_health(
        &self,
        outbound_id: OutboundId,
        revision: u64,
        state: nonproxy_proto::events::v1::RuntimeState,
        latency_ms: Option<u64>,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        self.outbound_health
            .update(outbound_id, revision, state, latency_ms, now_unix_ms)
    }

    pub(crate) fn outbound_health(
        &self,
        outbound: &OutboundReference,
        now_unix_ms: u64,
    ) -> Result<Option<OutboundHealthObservation>, GatewayError> {
        self.outbound_health
            .current(outbound.id(), outbound.revision(), now_unix_ms)
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

    pub async fn active_snapshot_version(&self) -> Result<Option<u64>, GatewayError> {
        self.database
            .run(|database| {
                Ok(database
                    .snapshots()
                    .active()?
                    .map(|record| record.artifact().snapshot_version()))
            })
            .await
    }

    pub async fn active_compiled_snapshot(
        &self,
    ) -> Result<Option<CompiledPolicySnapshot>, GatewayError> {
        self.database
            .run(|database| {
                let Some(record) = database.snapshots().active()? else {
                    return Ok(None);
                };
                let artifact = record.artifact();
                let (policies, capabilities, default_decision) =
                    snapshot_payload::decode(artifact.payload())?;
                let compiled = PolicyCompiler::compile(CompileRequest::new(
                    artifact.snapshot_version(),
                    artifact.created_at_unix_ms(),
                    default_decision,
                    policies,
                    capabilities,
                ))?;
                if compiled.metadata().content_hash() != artifact.content_hash() {
                    return Err(GatewayError::InvalidContract("活动策略快照内容哈希不一致"));
                }
                Ok(Some(compiled))
            })
            .await
    }

    pub async fn load_or_create_synthetic_dns_space(
        &self,
        proposed_ipv6_prefix: Ipv6Addr,
    ) -> Result<SyntheticAddressSpace, GatewayError> {
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                Ok(database
                    .synthetic_dns()
                    .load_or_create_space(proposed_ipv6_prefix, now)?)
            })
            .await
    }

    pub async fn synthetic_dns_binding(
        &self,
        space: SyntheticAddressSpace,
        domain: DomainName,
        family: SyntheticAddressFamily,
    ) -> Result<SyntheticDnsBinding, GatewayError> {
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                Ok(database
                    .synthetic_dns()
                    .get_or_create(space, &domain, family, now)?)
            })
            .await
    }

    pub async fn synthetic_dns_lookup(
        &self,
        space: SyntheticAddressSpace,
        address: IpAddr,
    ) -> Result<Option<SyntheticDnsBinding>, GatewayError> {
        let now = unix_time_ms()?;
        self.database
            .run(move |database| Ok(database.synthetic_dns().lookup(space, address, now)?))
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

impl PublishedSnapshot {
    pub(crate) const fn new(artifact: SnapshotArtifact, default_decision: DecisionSpec) -> Self {
        Self {
            artifact,
            default_decision,
        }
    }

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
