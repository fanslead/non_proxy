use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use nonproxy_model::OutboundId;
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::OutboundReference;

use crate::{Gateway, GatewayError};

const OUTBOUND_HEALTH_STALE_AFTER_MS: u64 = 60_000;
const MAXIMUM_TRACKED_OUTBOUNDS: usize = 512;
const HEALTH_TRANSITION_THRESHOLD: u8 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboundHealthObservation {
    pub state: RuntimeState,
    pub latency_ms: Option<u64>,
    pub observed_at_unix_ms: u64,
}

#[derive(Clone)]
pub struct OutboundHealthRegistry {
    state: Arc<Mutex<HashMap<OutboundId, StoredOutboundHealth>>>,
}

#[derive(Clone, Debug)]
struct StoredOutboundHealth {
    revision: u64,
    observation: OutboundHealthObservation,
    stable_state: Option<RuntimeState>,
    success_streak: u8,
    failure_streak: u8,
}

impl OutboundHealthRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn update(
        &self,
        outbound_id: OutboundId,
        revision: u64,
        state: RuntimeState,
        latency_ms: Option<u64>,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let mut health = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("出口健康状态"))?;
        if health.get(&outbound_id).is_some_and(|previous| {
            previous.revision > revision
                || (previous.revision == revision
                    && previous.observation.observed_at_unix_ms > now_unix_ms)
        }) {
            return Ok(());
        }
        health.retain(|_, value| {
            now_unix_ms.saturating_sub(value.observation.observed_at_unix_ms)
                <= OUTBOUND_HEALTH_STALE_AFTER_MS
        });
        if health.len() >= MAXIMUM_TRACKED_OUTBOUNDS && !health.contains_key(&outbound_id) {
            let oldest = health
                .iter()
                .min_by_key(|(_, value)| value.observation.observed_at_unix_ms)
                .map(|(id, _)| id.clone());
            if let Some(oldest) = oldest {
                health.remove(&oldest);
            }
        }
        let previous = health.remove(&outbound_id);
        let (stable_state, success_streak, failure_streak) = transition(
            previous.as_ref().filter(|value| value.revision == revision),
            state,
        );
        health.insert(
            outbound_id,
            StoredOutboundHealth {
                revision,
                observation: OutboundHealthObservation {
                    state,
                    latency_ms,
                    observed_at_unix_ms: now_unix_ms,
                },
                stable_state,
                success_streak,
                failure_streak,
            },
        );
        Ok(())
    }

    pub fn current(
        &self,
        outbound_id: &OutboundId,
        revision: u64,
        now_unix_ms: u64,
    ) -> Result<Option<OutboundHealthObservation>, GatewayError> {
        let health = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("出口健康状态"))?;
        Ok(health.get(outbound_id).and_then(|value| {
            (value.revision == revision
                && now_unix_ms.saturating_sub(value.observation.observed_at_unix_ms)
                    <= OUTBOUND_HEALTH_STALE_AFTER_MS)
                .then(|| value.observation.clone())
        }))
    }

    pub fn current_stable(
        &self,
        outbound_id: &OutboundId,
        revision: u64,
        now_unix_ms: u64,
    ) -> Result<Option<OutboundHealthObservation>, GatewayError> {
        let health = self
            .state
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("出口健康状态"))?;
        Ok(health.get(outbound_id).and_then(|value| {
            let fresh = value.revision == revision
                && now_unix_ms.saturating_sub(value.observation.observed_at_unix_ms)
                    <= OUTBOUND_HEALTH_STALE_AFTER_MS;
            fresh
                .then_some(value.stable_state)
                .flatten()
                .map(|state| OutboundHealthObservation {
                    state,
                    latency_ms: (state == value.observation.state)
                        .then_some(value.observation.latency_ms)
                        .flatten(),
                    observed_at_unix_ms: value.observation.observed_at_unix_ms,
                })
        }))
    }
}

fn transition(
    previous: Option<&StoredOutboundHealth>,
    state: RuntimeState,
) -> (Option<RuntimeState>, u8, u8) {
    let mut stable = previous.and_then(|value| value.stable_state);
    let mut successes = previous.map_or(0, |value| value.success_streak);
    let mut failures = previous.map_or(0, |value| value.failure_streak);
    match state {
        RuntimeState::Ready => {
            successes = successes.saturating_add(1);
            failures = 0;
            if successes >= HEALTH_TRANSITION_THRESHOLD {
                stable = Some(RuntimeState::Ready);
            }
        }
        RuntimeState::Failed => {
            failures = failures.saturating_add(1);
            successes = 0;
            if failures >= HEALTH_TRANSITION_THRESHOLD {
                stable = Some(RuntimeState::Failed);
            }
        }
        _ => {
            stable = None;
            successes = 0;
            failures = 0;
        }
    }
    (stable, successes, failures)
}

impl Default for OutboundHealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Gateway {
    pub(crate) fn stable_outbound_health(
        &self,
        outbound: &OutboundReference,
        now_unix_ms: u64,
    ) -> Result<Option<OutboundHealthObservation>, GatewayError> {
        self.outbound_health
            .current_stable(outbound.id(), outbound.revision(), now_unix_ms)
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_model::OutboundId;
    use nonproxy_proto::events::v1::RuntimeState;

    use super::OutboundHealthRegistry;

    #[test]
    fn health_requires_current_revision_and_fresh_observation() {
        let registry = OutboundHealthRegistry::new();
        let outbound_id = outbound_id();
        let updated = registry.update(
            outbound_id.clone(),
            4,
            RuntimeState::Ready,
            Some(27),
            10_000,
        );
        assert!(updated.is_ok());

        let current = registry.current(&outbound_id, 4, 10_500);
        assert!(matches!(
            current,
            Ok(Some(value))
                if value.state == RuntimeState::Ready
                    && value.latency_ms == Some(27)
                    && value.observed_at_unix_ms == 10_000
        ));
        assert!(matches!(
            registry.current(&outbound_id, 5, 10_500),
            Ok(None)
        ));
        assert!(matches!(
            registry.current(&outbound_id, 4, 70_000),
            Ok(Some(value)) if value.latency_ms == Some(27)
        ));
        assert!(matches!(
            registry.current(&outbound_id, 4, 70_001),
            Ok(None)
        ));
    }

    #[test]
    fn registry_evicts_oldest_observation_at_capacity() {
        let registry = OutboundHealthRegistry::new();
        for index in 0..=super::MAXIMUM_TRACKED_OUTBOUNDS {
            let id = match OutboundId::new(format!("proxy-{index}")) {
                Ok(value) => value,
                Err(error) => panic!("容量测试出口 ID 创建失败: {error}"),
            };
            let updated = registry.update(
                id,
                1,
                RuntimeState::Ready,
                Some(10),
                10_000 + u64::try_from(index).map_or(0, |value| value),
            );
            assert!(updated.is_ok());
        }

        let oldest = match OutboundId::new("proxy-0") {
            Ok(value) => value,
            Err(error) => panic!("容量测试最旧出口 ID 创建失败: {error}"),
        };
        let newest = match OutboundId::new(format!("proxy-{}", super::MAXIMUM_TRACKED_OUTBOUNDS)) {
            Ok(value) => value,
            Err(error) => panic!("容量测试最新出口 ID 创建失败: {error}"),
        };

        assert!(matches!(registry.current(&oldest, 1, 11_000), Ok(None)));
        assert!(matches!(
            registry.current(&newest, 1, 11_000),
            Ok(Some(value)) if value.state == RuntimeState::Ready
        ));
    }

    #[test]
    fn stable_health_requires_two_matching_observations_and_resets_by_revision() {
        let registry = OutboundHealthRegistry::new();
        let outbound_id = outbound_id();

        assert!(
            registry
                .update(
                    outbound_id.clone(),
                    4,
                    RuntimeState::Ready,
                    Some(20),
                    10_000
                )
                .is_ok()
        );
        assert!(matches!(
            registry.current_stable(&outbound_id, 4, 10_100),
            Ok(None)
        ));
        assert!(
            registry
                .update(
                    outbound_id.clone(),
                    4,
                    RuntimeState::Ready,
                    Some(18),
                    11_000
                )
                .is_ok()
        );
        assert!(matches!(
            registry.current_stable(&outbound_id, 4, 11_100),
            Ok(Some(value)) if value.state == RuntimeState::Ready && value.latency_ms == Some(18)
        ));

        assert!(
            registry
                .update(outbound_id.clone(), 4, RuntimeState::Failed, None, 12_000)
                .is_ok()
        );
        assert!(matches!(
            registry.current_stable(&outbound_id, 4, 12_100),
            Ok(Some(value)) if value.state == RuntimeState::Ready
        ));
        assert!(
            registry
                .update(outbound_id.clone(), 4, RuntimeState::Failed, None, 13_000)
                .is_ok()
        );
        assert!(matches!(
            registry.current_stable(&outbound_id, 4, 13_100),
            Ok(Some(value)) if value.state == RuntimeState::Failed
        ));

        assert!(
            registry
                .update(
                    outbound_id.clone(),
                    5,
                    RuntimeState::Ready,
                    Some(17),
                    14_000
                )
                .is_ok()
        );
        assert!(matches!(
            registry.current_stable(&outbound_id, 5, 14_100),
            Ok(None)
        ));
        assert!(matches!(
            registry.current_stable(&outbound_id, 4, 14_100),
            Ok(None)
        ));
    }

    #[test]
    fn stable_health_becomes_unknown_when_the_latest_probe_is_stale() {
        let registry = OutboundHealthRegistry::new();
        let outbound_id = outbound_id();
        for observed_at in [10_000, 11_000] {
            assert!(
                registry
                    .update(
                        outbound_id.clone(),
                        1,
                        RuntimeState::Ready,
                        Some(10),
                        observed_at,
                    )
                    .is_ok()
            );
        }

        assert!(matches!(
            registry.current_stable(&outbound_id, 1, 71_000),
            Ok(Some(_))
        ));
        assert!(matches!(
            registry.current_stable(&outbound_id, 1, 71_001),
            Ok(None)
        ));
    }

    #[test]
    fn late_probe_completion_cannot_replace_a_newer_revision_or_observation() {
        let registry = OutboundHealthRegistry::new();
        let outbound_id = outbound_id();
        assert!(
            registry
                .update(
                    outbound_id.clone(),
                    2,
                    RuntimeState::Ready,
                    Some(12),
                    20_000,
                )
                .is_ok()
        );
        assert!(
            registry
                .update(outbound_id.clone(), 2, RuntimeState::Failed, None, 19_000,)
                .is_ok()
        );
        assert!(
            registry
                .update(outbound_id.clone(), 1, RuntimeState::Failed, None, 21_000,)
                .is_ok()
        );

        assert!(matches!(
            registry.current(&outbound_id, 2, 20_100),
            Ok(Some(value)) if value.state == RuntimeState::Ready
                && value.observed_at_unix_ms == 20_000
        ));
    }

    fn outbound_id() -> OutboundId {
        match OutboundId::new("primary") {
            Ok(value) => value,
            Err(error) => panic!("测试出口 ID 创建失败: {error}"),
        }
    }
}
