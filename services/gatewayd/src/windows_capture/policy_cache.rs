use std::{sync::Arc, time::Duration};

use nonproxy_policy::CompiledPolicySnapshot;
use nonproxy_policy_compiler::{CompileRequest, PolicyCompiler};
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{ProviderAck, SnapshotStatus};
use tokio::{
    sync::{Mutex, RwLock, watch},
    time::{MissedTickBehavior, interval},
};

use crate::{Gateway, GatewayError, clock::unix_time_ms, provider_requirements, snapshot_payload};

const REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const PROVIDER_ID: &str = "windows-wfp";

#[derive(Clone)]
pub struct WindowsPolicyCache {
    gateway: Gateway,
    current: Arc<RwLock<Option<Arc<CompiledPolicySnapshot>>>>,
    provider_generation: u64,
    processed_candidate: Arc<Mutex<Option<u64>>>,
}

impl WindowsPolicyCache {
    pub async fn load(gateway: Gateway) -> Result<Self, GatewayError> {
        let provider_generation = gateway
            .next_provider_generation(PROVIDER_ID.to_owned())
            .await?;
        gateway.mark_provider_registered(PROVIDER_ID, provider_generation, unix_time_ms()?)?;
        let current = gateway.active_compiled_snapshot().await?.map(Arc::new);
        let cache = Self {
            gateway,
            current: Arc::new(RwLock::new(current)),
            provider_generation,
            processed_candidate: Arc::new(Mutex::new(None)),
        };
        cache.refresh_provider().await?;
        cache.refresh().await?;
        Ok(cache)
    }

    pub async fn current(&self) -> Option<Arc<CompiledPolicySnapshot>> {
        self.current.read().await.clone()
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
            PROVIDER_ID,
            self.provider_generation,
            state,
            active_snapshot_version,
            unix_time_ms()?,
        )
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
        let (policies, capabilities, default_decision) =
            snapshot_payload::decode(artifact.payload())?;
        let compiled = PolicyCompiler::compile(CompileRequest::new(
            version,
            artifact.created_at_unix_ms(),
            default_decision,
            policies,
            capabilities,
        ));
        let now = unix_time_ms()?;
        let acknowledgement = match compiled {
            Ok(snapshot) if snapshot.metadata().content_hash() == artifact.content_hash() => {
                ProviderAck::loaded(
                    PROVIDER_ID,
                    self.provider_generation,
                    *artifact.content_hash(),
                    now,
                )?
            }
            Ok(_) => ProviderAck::rejected(
                PROVIDER_ID,
                self.provider_generation,
                *artifact.content_hash(),
                "NP_WINDOWS_WFP_SNAPSHOT_HASH_MISMATCH",
                now,
            )?,
            Err(_) => ProviderAck::rejected(
                PROVIDER_ID,
                self.provider_generation,
                *artifact.content_hash(),
                "NP_WINDOWS_WFP_SNAPSHOT_REJECTED",
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
