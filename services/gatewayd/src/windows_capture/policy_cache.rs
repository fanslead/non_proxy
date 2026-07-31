use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nonproxy_policy::CompiledPolicySnapshot;
use nonproxy_policy_compiler::PolicyCompiler;
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{ProviderAck, SnapshotStatus};
use tokio::{
    sync::{Mutex, RwLock, watch},
    time::{MissedTickBehavior, interval},
};

use crate::{Gateway, GatewayError, clock::unix_time_ms, provider_requirements, snapshot_payload};

const REFRESH_INTERVAL: Duration = Duration::from_millis(250);
#[derive(Clone)]
pub struct WindowsPolicyCache {
    gateway: Gateway,
    current: Arc<RwLock<Option<Arc<CompiledPolicySnapshot>>>>,
    provider_id: &'static str,
    provider_generation: u64,
    processed_candidate: Arc<Mutex<Option<u64>>>,
    acknowledgements_enabled: Arc<AtomicBool>,
}

impl WindowsPolicyCache {
    pub async fn load(
        gateway: Gateway,
        provider_id: &'static str,
        acknowledgements_enabled: bool,
    ) -> Result<Self, GatewayError> {
        let provider_generation = gateway
            .next_provider_generation(provider_id.to_owned())
            .await?;
        gateway.mark_provider_registered(provider_id, provider_generation, unix_time_ms()?)?;
        let current = gateway.active_compiled_snapshot().await?.map(Arc::new);
        let cache = Self {
            gateway,
            current: Arc::new(RwLock::new(current)),
            provider_id,
            provider_generation,
            processed_candidate: Arc::new(Mutex::new(None)),
            acknowledgements_enabled: Arc::new(AtomicBool::new(acknowledgements_enabled)),
        };
        cache.refresh_provider().await?;
        cache.refresh().await?;
        Ok(cache)
    }

    pub async fn current(&self) -> Option<Arc<CompiledPolicySnapshot>> {
        self.current.read().await.clone()
    }

    #[must_use]
    pub const fn provider_generation(&self) -> u64 {
        self.provider_generation
    }

    pub async fn refresh_until_shutdown(
        self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), GatewayError> {
        let mut ticker = interval(REFRESH_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
                _ = ticker.tick() => {
                    // 短暂数据库错误不能清空上一份已验证快照或停止整机数据面。
                    let _provider_result = self.refresh_provider().await;
                    let _refresh_result = self.refresh().await;
                }
            }
        }
    }

    pub fn report_health(
        &self,
        state: RuntimeState,
        active_snapshot_version: u64,
    ) -> Result<(), GatewayError> {
        self.gateway.report_provider_health(
            self.provider_id,
            self.provider_generation,
            state,
            active_snapshot_version,
            unix_time_ms()?,
        )
    }

    pub async fn enable_acknowledgements(&self) -> Result<(), GatewayError> {
        self.set_acknowledgements_enabled(true).await
    }

    pub async fn disable_acknowledgements(&self) {
        self.acknowledgements_enabled
            .store(false, Ordering::Release);
    }

    async fn set_acknowledgements_enabled(&self, enabled: bool) -> Result<(), GatewayError> {
        self.acknowledgements_enabled
            .store(enabled, Ordering::Release);
        if enabled {
            self.refresh_provider().await
        } else {
            Ok(())
        }
    }

    async fn refresh(&self) -> Result<(), GatewayError> {
        let active_version = self.gateway.active_snapshot_version().await?;
        let current_version = self
            .current
            .read()
            .await
            .as_ref()
            .map(|snapshot| snapshot.metadata().snapshot_version());
        if active_version == current_version {
            return Ok(());
        }
        let next = self.gateway.active_compiled_snapshot().await?.map(Arc::new);
        *self.current.write().await = next;
        Ok(())
    }

    async fn refresh_provider(&self) -> Result<(), GatewayError> {
        if !self.acknowledgements_enabled.load(Ordering::Acquire) {
            return Ok(());
        }
        let known = self
            .current
            .read()
            .await
            .as_ref()
            .map_or(0, |snapshot| snapshot.metadata().snapshot_version());
        let Some(candidate) = self.gateway.provider_snapshot(known).await? else {
            return Ok(());
        };
        let record = candidate.record();
        if record.status() != SnapshotStatus::Pending {
            return Ok(());
        }
        let version = record.artifact().snapshot_version();
        if self.processed_candidate.lock().await.as_ref() == Some(&version) {
            return Ok(());
        }
        let artifact = record.artifact();
        let decoded = snapshot_payload::decode_versioned(artifact.payload())?;
        let compiled = PolicyCompiler::compile(
            decoded.into_compile_request(version, artifact.created_at_unix_ms()),
        );
        let now = unix_time_ms()?;
        let acknowledgement = match compiled {
            Ok(snapshot) if snapshot.metadata().content_hash() == artifact.content_hash() => {
                ProviderAck::loaded(
                    self.provider_id,
                    self.provider_generation,
                    *artifact.content_hash(),
                    now,
                )?
            }
            Ok(_) => ProviderAck::rejected(
                self.provider_id,
                self.provider_generation,
                *artifact.content_hash(),
                "NP_WINDOWS_SNAPSHOT_HASH_MISMATCH",
                now,
            )?,
            Err(_) => ProviderAck::rejected(
                self.provider_id,
                self.provider_generation,
                *artifact.content_hash(),
                "NP_WINDOWS_SNAPSHOT_REJECTED",
                now,
            )?,
        };
        let required = provider_requirements::required_provider_ids()
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        self.gateway
            .acknowledge_provider_snapshot(version, acknowledgement, required)
            .await?;
        *self.processed_candidate.lock().await = Some(version);
        Ok(())
    }
}
