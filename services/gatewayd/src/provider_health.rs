use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use nonproxy_proto::events::v1::RuntimeState;

use crate::GatewayError;

const PROVIDER_HEALTH_STALE_AFTER_MS: u64 = 15_000;

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
        self.update(
            provider_id,
            generation,
            RuntimeState::Starting,
            0,
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
}

impl Default for ProviderHealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::events::v1::RuntimeState;

    use super::ProviderHealthRegistry;

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
}
