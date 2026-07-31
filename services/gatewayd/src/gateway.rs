use std::{
    net::{IpAddr, Ipv6Addr},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nonproxy_dns::{SyntheticAddressFamily, SyntheticAddressSpace};
use nonproxy_model::{DomainName, NetworkProfileId, OutboundId, Policy, PolicyId};
use nonproxy_policy::CompiledPolicySnapshot;
use nonproxy_policy_compiler::{CompileCapabilities, PolicyCompiler};
use nonproxy_storage::{
    NetworkProfileReference, OutboundReference, PolicyDatabase, RoutingSettings, SnapshotRecord,
    SyntheticDnsBinding,
};
use tokio::sync::Mutex;

use crate::{
    GatewayError,
    clock::unix_time_ms,
    database_executor::DatabaseExecutor,
    decision_snapshot_cache::DecisionSnapshotCache,
    decision_telemetry::DecisionTelemetryRegistry,
    event_hub::EventHub,
    outbound_health::{OutboundHealthObservation, OutboundHealthRegistry},
    provider_health::ProviderHealthRegistry,
    provider_requirements,
    routing_gateway::decision_for_route,
    runtime_policy::{RuntimePolicyCatalog, RuntimePolicyRecord, build_runtime_catalog},
    snapshot_builder::{SnapshotBuildIdentity, build_snapshot},
    snapshot_payload,
    snapshot_types::{ProviderSnapshot, PublishedSnapshot},
    system_policies::SystemPolicyConfig,
};

#[derive(Clone)]
pub struct Gateway {
    pub(crate) database: DatabaseExecutor,
    capabilities: CompileCapabilities,
    pub(crate) system_policy_config: SystemPolicyConfig,
    pub(crate) mutation_gate: Arc<Mutex<()>>,
    events: EventHub,
    outbound_health: OutboundHealthRegistry,
    provider_health: ProviderHealthRegistry,
    pub(crate) decision_snapshots: DecisionSnapshotCache,
    decision_telemetry: DecisionTelemetryRegistry,
    system_snapshot_ready: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct GatewayStatus {
    pub active: Option<SnapshotRecord>,
    pub pending: Option<SnapshotRecord>,
    pub policy_count: usize,
    pub data_plane_ready: bool,
    pub routing: RoutingSettings,
    pub dropped_decision_events: u64,
}

impl Gateway {
    pub async fn open(
        database_path: impl AsRef<Path>,
        capabilities: CompileCapabilities,
    ) -> Result<Self, GatewayError> {
        Self::open_with_system_policy(database_path, capabilities, SystemPolicyConfig::default())
            .await
    }

    pub(crate) async fn open_with_system_policy(
        database_path: impl AsRef<Path>,
        capabilities: CompileCapabilities,
        system_policy_config: SystemPolicyConfig,
    ) -> Result<Self, GatewayError> {
        let path = database_path.as_ref().to_path_buf();
        let now = unix_time_ms()?;
        let database = tokio::task::spawn_blocking(move || PolicyDatabase::open(path, now))
            .await
            .map_err(|error| GatewayError::DatabaseTask(error.to_string()))??;
        Ok(Self::new_with_system_policy(
            database,
            capabilities,
            system_policy_config,
        ))
    }

    #[must_use]
    pub fn new(database: PolicyDatabase, capabilities: CompileCapabilities) -> Self {
        Self::new_with_system_policy(database, capabilities, SystemPolicyConfig::default())
    }

    #[must_use]
    pub(crate) fn new_with_system_policy(
        database: PolicyDatabase,
        capabilities: CompileCapabilities,
        system_policy_config: SystemPolicyConfig,
    ) -> Self {
        Self {
            database: DatabaseExecutor::new(database),
            capabilities,
            system_policy_config,
            mutation_gate: Arc::new(Mutex::new(())),
            events: EventHub::new(),
            outbound_health: OutboundHealthRegistry::new(),
            provider_health: ProviderHealthRegistry::new(),
            decision_snapshots: DecisionSnapshotCache::default(),
            decision_telemetry: DecisionTelemetryRegistry::default(),
            system_snapshot_ready: Arc::new(AtomicBool::new(true)),
        }
    }

    #[must_use]
    pub const fn capabilities(&self) -> &CompileCapabilities {
        &self.capabilities
    }

    #[must_use]
    pub(crate) fn system_snapshot_ready(&self) -> bool {
        self.system_snapshot_ready.load(Ordering::Acquire)
    }

    pub(crate) fn set_system_snapshot_ready(&self, ready: bool) {
        self.system_snapshot_ready.store(ready, Ordering::Release);
    }

    #[must_use]
    pub const fn events(&self) -> &EventHub {
        &self.events
    }

    #[cfg(any(test, windows))]
    pub(crate) fn record_dropped_decisions(&self, count: u64) {
        self.decision_telemetry.record_dropped(count);
    }

    pub(crate) fn record_provider_dropped_decisions(
        &self,
        provider_id: &str,
        provider_generation: u64,
        batch_id: &str,
        count: u64,
    ) {
        self.decision_telemetry.record_provider_dropped(
            provider_id,
            provider_generation,
            batch_id,
            count,
        );
    }

    pub async fn status(&self) -> Result<GatewayStatus, GatewayError> {
        let dropped_decision_events = self.decision_telemetry.dropped_events();
        let mut status = self
            .database
            .run(move |database| {
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
                    dropped_decision_events,
                })
            })
            .await?;
        if let Some(active) = status.active.as_ref() {
            status.data_plane_ready = self.system_snapshot_ready()
                && self.provider_health.all_ready(
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
        let system_policy_config = self.system_policy_config.clone();
        self.database
            .run(move |database| {
                let policies = database.policies().list()?;
                let outbounds = database.outbounds().list()?;
                let network_profiles = database.network_profiles().list()?;
                let routing = database.routing_settings().get()?;
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let snapshot_version = current
                    .checked_add(1)
                    .ok_or(GatewayError::SnapshotVersionExhausted)?;
                let published = build_snapshot(
                    capabilities,
                    &policies,
                    &outbounds,
                    &network_profiles,
                    decision_for_route(routing.route())?,
                    SnapshotBuildIdentity::new(snapshot_version, now_unix_ms),
                    &system_policy_config,
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
                Ok(Some(ProviderSnapshot::new(record, default_decision)))
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
                let decoded = snapshot_payload::decode_versioned(artifact.payload())?;
                let compiled = PolicyCompiler::compile(decoded.into_compile_request(
                    artifact.snapshot_version(),
                    artifact.created_at_unix_ms(),
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
}
