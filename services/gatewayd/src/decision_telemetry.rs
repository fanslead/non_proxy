use std::{
    collections::{HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const PROVIDER_BATCH_HISTORY_CAPACITY: usize = 4_096;

#[derive(Clone, Default)]
pub struct DecisionTelemetryRegistry {
    dropped_events: Arc<AtomicU64>,
    provider_batches: Arc<Mutex<ProviderBatchHistory>>,
}

#[derive(Default)]
struct ProviderBatchHistory {
    seen: HashSet<String>,
    order: VecDeque<String>,
}

impl DecisionTelemetryRegistry {
    pub fn record_dropped(&self, count: u64) {
        if count == 0 {
            return;
        }
        let _previous =
            self.dropped_events
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    Some(value.saturating_add(count))
                });
    }

    pub fn record_provider_dropped(
        &self,
        provider_id: &str,
        provider_generation: u64,
        batch_id: &str,
        count: u64,
    ) {
        if count == 0 {
            return;
        }
        let key = format!("{provider_id}:{provider_generation}:{batch_id}");
        let first_observation = {
            let mut history = match self.provider_batches.lock() {
                Ok(value) => value,
                Err(poisoned) => poisoned.into_inner(),
            };
            if history.seen.contains(&key) {
                false
            } else {
                if history.order.len() == PROVIDER_BATCH_HISTORY_CAPACITY
                    && let Some(expired) = history.order.pop_front()
                {
                    history.seen.remove(&expired);
                }
                history.seen.insert(key.clone());
                history.order.push_back(key);
                true
            }
        };
        if first_observation {
            self.record_dropped(count);
        }
    }

    #[must_use]
    pub fn dropped_events(&self) -> u64 {
        self.dropped_events.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::DecisionTelemetryRegistry;

    #[test]
    fn dropped_count_saturates_instead_of_wrapping() {
        let registry = DecisionTelemetryRegistry::default();
        registry.record_dropped(u64::MAX);
        registry.record_dropped(1);

        assert_eq!(registry.dropped_events(), u64::MAX);
    }

    #[test]
    fn provider_batch_retry_does_not_double_count_dropped_events() {
        let registry = DecisionTelemetryRegistry::default();

        registry.record_provider_dropped("transparent-proxy", 3, "batch-1", 7);
        registry.record_provider_dropped("transparent-proxy", 3, "batch-1", 7);
        registry.record_provider_dropped("transparent-proxy", 3, "batch-2", 2);

        assert_eq!(registry.dropped_events(), 9);
    }
}
