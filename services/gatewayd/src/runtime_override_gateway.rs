use nonproxy_model::{OutboundId, RuntimeOverrideMode, RuntimeRoutingOverride};
use nonproxy_policy_compiler::MAX_RUNTIME_OVERRIDE_DURATION_MS;
use nonproxy_storage::SnapshotRecord;

use crate::{
    Gateway, GatewayError, PublishedSnapshot,
    clock::unix_time_ms,
    snapshot_builder::{SnapshotBuildIdentity, SnapshotRoutingState, rebuild_snapshot},
    snapshot_payload,
};

const MIN_RUNTIME_OVERRIDE_DURATION_MS: u64 = 1_000;

#[derive(Clone, Debug)]
pub struct RuntimeOverrideStatus {
    pub active: Option<RuntimeRoutingOverride>,
    pub pending: Option<RuntimeRoutingOverride>,
    pub active_snapshot_version: Option<u64>,
    pub pending_snapshot_version: Option<u64>,
    pub pending_clears_override: bool,
}

impl Gateway {
    pub async fn runtime_override_status(&self) -> Result<RuntimeOverrideStatus, GatewayError> {
        let now = unix_time_ms()?;
        self.database
            .run(move |database| {
                let active_record = database.snapshots().active()?;
                let pending_record = database.snapshots().pending()?;
                let active = effective_override(active_record.as_ref(), now)?;
                let pending = effective_override(pending_record.as_ref(), now)?;
                Ok(RuntimeOverrideStatus {
                    active_snapshot_version: snapshot_version(active_record.as_ref()),
                    pending_snapshot_version: snapshot_version(pending_record.as_ref()),
                    pending_clears_override: pending_record.is_some()
                        && pending.is_none()
                        && active.is_some(),
                    active,
                    pending,
                })
            })
            .await
    }

    pub async fn stage_runtime_override(
        &self,
        mode: RuntimeOverrideMode,
        outbound_id: Option<OutboundId>,
        duration_ms: u64,
        expected_active_snapshot_version: u64,
    ) -> Result<PublishedSnapshot, GatewayError> {
        if !(MIN_RUNTIME_OVERRIDE_DURATION_MS..=MAX_RUNTIME_OVERRIDE_DURATION_MS)
            .contains(&duration_ms)
        {
            return Err(GatewayError::RuntimeOverrideDurationInvalid);
        }
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let expires_at = now
            .checked_add(duration_ms)
            .ok_or(GatewayError::ClockOverflow)?;
        let runtime_override = RuntimeRoutingOverride::new(mode, outbound_id, expires_at)?;
        self.rebuild_with_runtime_override(
            Some(runtime_override),
            expected_active_snapshot_version,
            now,
            false,
        )
        .await
    }

    pub async fn clear_runtime_override(
        &self,
        expected_active_snapshot_version: u64,
    ) -> Result<PublishedSnapshot, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.rebuild_with_runtime_override(None, expected_active_snapshot_version, now, true)
            .await
    }

    async fn rebuild_with_runtime_override(
        &self,
        runtime_override: Option<RuntimeRoutingOverride>,
        expected_active_snapshot_version: u64,
        now_unix_ms: u64,
        require_current_override: bool,
    ) -> Result<PublishedSnapshot, GatewayError> {
        let system_policy_config = self.system_policy_config.clone();
        self.database
            .run(move |database| {
                let active = database
                    .snapshots()
                    .active()?
                    .ok_or(GatewayError::RuntimeOverrideActiveSnapshotMissing)?;
                if require_current_override
                    && snapshot_payload::effective_runtime_override(
                        active.artifact().payload(),
                        now_unix_ms,
                    )?
                    .is_none()
                {
                    return Err(GatewayError::RuntimeOverrideNotActive);
                }
                let decoded = snapshot_payload::decode_versioned(active.artifact().payload())?;
                let network_profiles = if decoded.includes_network_profiles {
                    decoded.network_profiles
                } else {
                    database
                        .network_profiles()
                        .list()?
                        .iter()
                        .map(nonproxy_storage::NetworkProfileReference::binding)
                        .collect()
                };
                let next = database
                    .snapshots()
                    .latest_version()?
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or(GatewayError::SnapshotVersionExhausted)?;
                let published = rebuild_snapshot(
                    decoded.capabilities,
                    &decoded.policies,
                    &network_profiles,
                    SnapshotRoutingState::new(decoded.default_decision, runtime_override),
                    SnapshotBuildIdentity::new(next, now_unix_ms),
                    &system_policy_config,
                )?;
                database.snapshots().stage_for_active_snapshot(
                    published.artifact(),
                    expected_active_snapshot_version,
                )?;
                Ok(published)
            })
            .await
    }
}

fn effective_override(
    record: Option<&SnapshotRecord>,
    now_unix_ms: u64,
) -> Result<Option<RuntimeRoutingOverride>, GatewayError> {
    record
        .map(|record| {
            snapshot_payload::effective_runtime_override(record.artifact().payload(), now_unix_ms)
        })
        .transpose()
        .map(Option::flatten)
}

fn snapshot_version(record: Option<&SnapshotRecord>) -> Option<u64> {
    record.map(|record| record.artifact().snapshot_version())
}
