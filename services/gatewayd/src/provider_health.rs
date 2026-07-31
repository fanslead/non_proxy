use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use nonproxy_proto::{
    common::v1::{ComponentKind, ErrorDetail},
    events::v1::RuntimeState,
};

use crate::GatewayError;

const PROVIDER_HEALTH_STALE_AFTER_MS: u64 = 15_000;
const MAX_HEALTH_ERROR_CODE_LENGTH: usize = 128;

#[derive(Clone)]
pub struct ProviderHealthRegistry {
    state: Arc<Mutex<HashMap<String, ProviderHealth>>>,
}

#[derive(Clone, Debug)]
struct ProviderHealth {
    generation: u64,
    state: RuntimeState,
    active_snapshot_version: u64,
    observed_at_unix_ms: u64,
    error: Option<ProviderHealthError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderHealthError {
    code: String,
    retryable: bool,
}

impl ProviderHealthError {
    pub fn new(code: String, retryable: bool) -> Result<Self, GatewayError> {
        if code.is_empty()
            || code.len() > MAX_HEALTH_ERROR_CODE_LENGTH
            || !code.starts_with("NP_")
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(GatewayError::InvalidRequest(
                "Provider 健康错误码必须是受限的 NP_ 稳定码",
            ));
        }
        Ok(Self { code, retryable })
    }

    fn detail(&self) -> ErrorDetail {
        ErrorDetail {
            code: self.code.clone(),
            message: "数据面组件报告运行异常。".to_owned(),
            retryable: self.retryable,
            metadata: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentHealthSnapshot {
    component: ComponentKind,
    state: RuntimeState,
    active_snapshot_version: u64,
    last_seen_at_unix_ms: Option<u64>,
    error: Option<ErrorDetail>,
}

impl ComponentHealthSnapshot {
    #[must_use]
    pub const fn component(&self) -> ComponentKind {
        self.component
    }

    #[must_use]
    pub const fn state(&self) -> RuntimeState {
        self.state
    }

    #[must_use]
    pub const fn active_snapshot_version(&self) -> u64 {
        self.active_snapshot_version
    }

    #[must_use]
    pub const fn last_seen_at_unix_ms(&self) -> Option<u64> {
        self.last_seen_at_unix_ms
    }

    #[must_use]
    pub const fn error(&self) -> Option<&ErrorDetail> {
        self.error.as_ref()
    }
}

#[derive(Clone, Debug)]
struct EvaluatedHealth {
    state: RuntimeState,
    active_snapshot_version: u64,
    last_seen_at_unix_ms: Option<u64>,
    error: Option<ErrorDetail>,
}

impl ProviderHealthRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn registered(
        &self,
        provider_id: &str,
        generation: u64,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        self.update_with_error(
            provider_id,
            generation,
            RuntimeState::Starting,
            0,
            None,
            now_unix_ms,
        )
    }

    pub fn update(
        &self,
        provider_id: &str,
        generation: u64,
        state: RuntimeState,
        active_snapshot_version: u64,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        self.update_with_error(
            provider_id,
            generation,
            state,
            active_snapshot_version,
            None,
            now_unix_ms,
        )
    }

    pub fn update_with_error(
        &self,
        provider_id: &str,
        generation: u64,
        state: RuntimeState,
        active_snapshot_version: u64,
        error: Option<ProviderHealthError>,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let mut health = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("Provider 健康状态"))?;
        if health
            .get(provider_id)
            .is_some_and(|current| current.generation > generation)
        {
            return Err(GatewayError::InvalidRequest("Provider generation 已过期"));
        }
        health.insert(
            provider_id.to_owned(),
            ProviderHealth {
                generation,
                state,
                active_snapshot_version,
                observed_at_unix_ms: now_unix_ms,
                error,
            },
        );
        Ok(())
    }

    pub fn all_ready(
        &self,
        required_provider_ids: &[&str],
        active_snapshot_version: u64,
        now_unix_ms: u64,
    ) -> Result<bool, GatewayError> {
        let health = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("Provider 健康状态"))?;
        Ok(required_provider_ids.iter().all(|provider_id| {
            health.get(*provider_id).is_some_and(|value| {
                value.state == RuntimeState::Ready
                    && value.active_snapshot_version == active_snapshot_version
                    && now_unix_ms.saturating_sub(value.observed_at_unix_ms)
                        <= PROVIDER_HEALTH_STALE_AFTER_MS
            })
        }))
    }

    pub fn component_snapshots(
        &self,
        required_provider_ids: &[&str],
        active_snapshot_version: u64,
        now_unix_ms: u64,
    ) -> Result<Vec<ComponentHealthSnapshot>, GatewayError> {
        let health = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("Provider 健康状态"))?;
        let mut snapshots = Vec::<ComponentHealthSnapshot>::new();
        for provider_id in required_provider_ids {
            let component = component_for_provider(provider_id);
            let evaluated = evaluate(
                health.get(*provider_id),
                active_snapshot_version,
                now_unix_ms,
            );
            if let Some(snapshot) = snapshots
                .iter_mut()
                .find(|snapshot| snapshot.component == component)
            {
                merge(snapshot, evaluated);
            } else {
                snapshots.push(ComponentHealthSnapshot {
                    component,
                    state: evaluated.state,
                    active_snapshot_version: evaluated.active_snapshot_version,
                    last_seen_at_unix_ms: evaluated.last_seen_at_unix_ms,
                    error: evaluated.error,
                });
            }
        }
        snapshots.sort_by_key(|snapshot| snapshot.component as i32);
        Ok(snapshots)
    }
}

impl Default for ProviderHealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub const fn component_for_provider(provider_id: &str) -> ComponentKind {
    match provider_id.as_bytes() {
        b"transparent-proxy" => ComponentKind::TransparentProxy,
        b"dns-proxy" => ComponentKind::DnsProxy,
        b"windows-wfp" | b"windows-dns" => ComponentKind::WindowsService,
        _ => ComponentKind::Gateway,
    }
}

fn evaluate(
    health: Option<&ProviderHealth>,
    active_snapshot_version: u64,
    now_unix_ms: u64,
) -> EvaluatedHealth {
    let Some(health) = health else {
        return EvaluatedHealth {
            state: RuntimeState::Starting,
            active_snapshot_version: 0,
            last_seen_at_unix_ms: None,
            error: Some(detail(
                "NP_PROVIDER_NOT_REGISTERED",
                "数据面组件尚未登记。",
                true,
            )),
        };
    };
    if now_unix_ms.saturating_sub(health.observed_at_unix_ms) > PROVIDER_HEALTH_STALE_AFTER_MS {
        return EvaluatedHealth {
            state: RuntimeState::Degraded,
            active_snapshot_version: health.active_snapshot_version,
            last_seen_at_unix_ms: Some(health.observed_at_unix_ms),
            error: Some(detail(
                "NP_PROVIDER_HEARTBEAT_STALE",
                "数据面组件心跳已过期。",
                true,
            )),
        };
    }
    if active_snapshot_version > 0
        && health.state == RuntimeState::Ready
        && health.active_snapshot_version != active_snapshot_version
    {
        return EvaluatedHealth {
            state: RuntimeState::Degraded,
            active_snapshot_version: health.active_snapshot_version,
            last_seen_at_unix_ms: Some(health.observed_at_unix_ms),
            error: Some(detail(
                "NP_PROVIDER_SNAPSHOT_NOT_ACTIVE",
                "数据面组件尚未确认当前策略快照。",
                true,
            )),
        };
    }
    EvaluatedHealth {
        state: health.state,
        active_snapshot_version: health.active_snapshot_version,
        last_seen_at_unix_ms: Some(health.observed_at_unix_ms),
        error: health
            .error
            .as_ref()
            .map(ProviderHealthError::detail)
            .or_else(|| default_state_error(health.state)),
    }
}

fn merge(snapshot: &mut ComponentHealthSnapshot, candidate: EvaluatedHealth) {
    let candidate_rank = state_rank(candidate.state);
    let current_rank = state_rank(snapshot.state);
    if candidate_rank > current_rank {
        snapshot.state = candidate.state;
        snapshot.error = candidate.error;
    } else if candidate_rank == current_rank && snapshot.error.is_none() {
        snapshot.error = candidate.error;
    }
    if snapshot.active_snapshot_version != candidate.active_snapshot_version {
        snapshot.active_snapshot_version = 0;
    }
    snapshot.last_seen_at_unix_ms = match (
        snapshot.last_seen_at_unix_ms,
        candidate.last_seen_at_unix_ms,
    ) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (current, None) | (None, current) => current,
    };
}

const fn state_rank(state: RuntimeState) -> u8 {
    match state {
        RuntimeState::Failed => 6,
        RuntimeState::Degraded => 5,
        RuntimeState::Stopped => 4,
        RuntimeState::Draining => 3,
        RuntimeState::Starting | RuntimeState::Unspecified => 2,
        RuntimeState::Ready => 1,
    }
}

fn default_state_error(state: RuntimeState) -> Option<ErrorDetail> {
    match state {
        RuntimeState::Degraded => {
            Some(detail("NP_PROVIDER_DEGRADED", "数据面组件报告降级。", true))
        }
        RuntimeState::Failed => Some(detail("NP_PROVIDER_FAILED", "数据面组件报告失败。", true)),
        RuntimeState::Stopped => Some(detail("NP_PROVIDER_STOPPED", "数据面组件已停止。", true)),
        _ => None,
    }
}

fn detail(code: &str, message: &str, retryable: bool) -> ErrorDetail {
    ErrorDetail {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable,
        metadata: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::{common::v1::ComponentKind, events::v1::RuntimeState};

    use super::{ProviderHealthError, ProviderHealthRegistry};

    #[test]
    fn requires_current_ready_health_from_every_provider() {
        let registry = ProviderHealthRegistry::new();
        assert!(registry.registered("transparent-proxy", 1, 1_000).is_ok());
        assert!(
            registry
                .update("transparent-proxy", 1, RuntimeState::Ready, 7, 2_000)
                .is_ok()
        );
        assert!(registry.registered("dns-proxy", 2, 2_000).is_ok());
        let required = ["transparent-proxy", "dns-proxy"];

        assert!(matches!(registry.all_ready(&required, 7, 2_100), Ok(false)));
        assert!(
            registry
                .update("dns-proxy", 2, RuntimeState::Ready, 7, 2_200)
                .is_ok()
        );
        assert!(matches!(registry.all_ready(&required, 7, 2_300), Ok(true)));
        assert!(matches!(
            registry.all_ready(&required, 7, 20_000),
            Ok(false)
        ));
    }

    #[test]
    fn component_snapshots_expose_missing_mismatch_and_stale_without_raw_provider_text() {
        let registry = ProviderHealthRegistry::new();
        let required = ["transparent-proxy", "dns-proxy"];
        assert!(
            registry
                .update_with_error(
                    "transparent-proxy",
                    1,
                    RuntimeState::Failed,
                    6,
                    ProviderHealthError::new("NP_TEST_PROVIDER_FAILURE".to_owned(), false).ok(),
                    2_000,
                )
                .is_ok()
        );

        let snapshots = registry.component_snapshots(&required, 7, 2_100);
        let Ok(snapshots) = snapshots else {
            panic!("Provider 组件快照失败: {snapshots:?}");
        };
        assert!(matches!(
            snapshots.as_slice(),
            [transparent, dns]
                if transparent.component() == ComponentKind::TransparentProxy
                    && transparent.state() == RuntimeState::Failed
                    && transparent.error().is_some_and(|error| {
                        error.code == "NP_TEST_PROVIDER_FAILURE"
                            && error.message == "数据面组件报告运行异常。"
                    })
                    && dns.component() == ComponentKind::DnsProxy
                    && dns.state() == RuntimeState::Starting
                    && dns.error().is_some_and(|error| {
                        error.code == "NP_PROVIDER_NOT_REGISTERED"
                    })
        ));

        assert!(
            registry
                .update("transparent-proxy", 1, RuntimeState::Ready, 6, 3_000)
                .is_ok()
        );
        assert!(matches!(
            registry.component_snapshots(&["transparent-proxy"], 7, 3_100),
            Ok(value)
                if value[0].state() == RuntimeState::Degraded
                    && value[0].error().is_some_and(|error| {
                        error.code == "NP_PROVIDER_SNAPSHOT_NOT_ACTIVE"
                    })
        ));
        assert!(matches!(
            registry.component_snapshots(&["transparent-proxy"], 6, 18_001),
            Ok(value)
                if value[0].state() == RuntimeState::Degraded
                    && value[0].error().is_some_and(|error| {
                        error.code == "NP_PROVIDER_HEARTBEAT_STALE"
                    })
        ));
    }

    #[test]
    fn windows_component_uses_the_worst_child_state() {
        let registry = ProviderHealthRegistry::new();
        assert!(
            registry
                .update("windows-wfp", 1, RuntimeState::Ready, 9, 1_000)
                .is_ok()
        );
        assert!(
            registry
                .update("windows-dns", 1, RuntimeState::Degraded, 9, 1_100)
                .is_ok()
        );

        assert!(matches!(
            registry.component_snapshots(&["windows-wfp", "windows-dns"], 9, 1_200),
            Ok(value)
                if value.len() == 1
                    && value[0].component() == ComponentKind::WindowsService
                    && value[0].state() == RuntimeState::Degraded
                    && value[0].last_seen_at_unix_ms() == Some(1_000)
        ));

        let partial = ProviderHealthRegistry::new();
        assert!(partial.registered("windows-wfp", 1, 1_000).is_ok());
        assert!(matches!(
            partial.component_snapshots(&["windows-wfp", "windows-dns"], 0, 1_100),
            Ok(value)
                if value.len() == 1
                    && value[0].state() == RuntimeState::Starting
                    && value[0].error().is_some_and(|error| {
                        error.code == "NP_PROVIDER_NOT_REGISTERED"
                    })
        ));
    }

    #[test]
    fn provider_health_error_requires_a_bounded_stable_code() {
        assert!(ProviderHealthError::new("NP_DNS_FAILED".to_owned(), true).is_ok());
        assert!(ProviderHealthError::new("dns failed".to_owned(), true).is_err());
        assert!(ProviderHealthError::new(format!("NP_{}", "A".repeat(126)), true).is_err());
    }
}
