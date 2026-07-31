use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use nonproxy_model::OutboundId;
use nonproxy_proto::events::v1::RuntimeState;

use crate::GatewayError;

const OUTBOUND_HEALTH_STALE_AFTER_MS: u64 = 60_000;
const MAXIMUM_TRACKED_OUTBOUNDS: usize = 512;

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
        health.insert(
            outbound_id,
            StoredOutboundHealth {
                revision,
                observation: OutboundHealthObservation {
                    state,
                    latency_ms,
                    observed_at_unix_ms: now_unix_ms,
                },
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
}

impl Default for OutboundHealthRegistry {
    fn default() -> Self {
        Self::new()
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

    fn outbound_id() -> OutboundId {
        match OutboundId::new("primary") {
            Ok(value) => value,
            Err(error) => panic!("测试出口 ID 创建失败: {error}"),
        }
    }
}
