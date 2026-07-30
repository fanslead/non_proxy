use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use nonproxy_proto::events::v1::EventEnvelope;
use tokio::sync::broadcast;

use crate::{GatewayError, clock::unix_time_ms};

const EVENT_BUFFER_CAPACITY: usize = 1_024;

#[derive(Clone)]
pub struct EventHub {
    inner: Arc<Mutex<EventState>>,
    sender: broadcast::Sender<EventEnvelope>,
}

struct EventState {
    next_sequence: u64,
    retained: VecDeque<EventEnvelope>,
}

impl EventHub {
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER_CAPACITY);
        Self {
            inner: Arc::new(Mutex::new(EventState {
                next_sequence: 1,
                retained: VecDeque::with_capacity(EVENT_BUFFER_CAPACITY),
            })),
            sender,
        }
    }

    pub fn publish(&self, mut event: EventEnvelope) -> Result<EventEnvelope, GatewayError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("事件"))?;
        event.sequence = state.next_sequence;
        event.event_id = format!("event-{}", state.next_sequence);
        event.occurred_at = Some(crate::clock::timestamp_from_unix_ms(unix_time_ms()?)?);
        state.next_sequence = state
            .next_sequence
            .checked_add(1)
            .ok_or(GatewayError::SnapshotVersionExhausted)?;
        if state.retained.len() == EVENT_BUFFER_CAPACITY {
            state.retained.pop_front();
        }
        state.retained.push_back(event.clone());
        let _subscriber_count = self.sender.send(event.clone());
        Ok(event)
    }

    pub fn subscribe(
        &self,
        after_sequence: u64,
    ) -> Result<(Vec<EventEnvelope>, broadcast::Receiver<EventEnvelope>), GatewayError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("事件"))?;
        let receiver = self.sender.subscribe();
        let backlog = state
            .retained
            .iter()
            .filter(|event| event.sequence > after_sequence)
            .cloned()
            .collect();
        Ok((backlog, receiver))
    }

    pub fn latest_sequence(&self) -> Result<u64, GatewayError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| GatewayError::StateLockPoisoned("事件"))?;
        Ok(state.next_sequence.saturating_sub(1))
    }
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}
