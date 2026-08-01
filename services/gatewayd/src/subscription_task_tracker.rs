use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

#[derive(Clone)]
pub(crate) struct SubscriptionTaskTracker {
    shared: Arc<SharedState>,
}

#[derive(Default)]
struct SharedState {
    state: Mutex<TaskState>,
    idle: Notify,
}

#[derive(Default)]
struct TaskState {
    accepting: bool,
    pending: usize,
}

impl SubscriptionTaskTracker {
    pub(crate) fn new() -> Self {
        Self {
            shared: Arc::new(SharedState {
                state: Mutex::new(TaskState {
                    accepting: true,
                    pending: 0,
                }),
                idle: Notify::new(),
            }),
        }
    }

    pub(crate) fn start(&self) -> Option<SubscriptionTaskGuard> {
        let mut state = self.lock_state();
        if !state.accepting {
            return None;
        }
        state.pending = state.pending.checked_add(1)?;
        Some(SubscriptionTaskGuard {
            shared: Arc::clone(&self.shared),
        })
    }

    pub(crate) fn close(&self) {
        self.lock_state().accepting = false;
    }

    pub(crate) async fn wait_idle(&self) {
        loop {
            let notified = self.shared.idle.notified();
            if self.lock_state().pending == 0 {
                return;
            }
            notified.await;
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, TaskState> {
        match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

pub(crate) struct SubscriptionTaskGuard {
    shared: Arc<SharedState>,
}

impl Drop for SubscriptionTaskGuard {
    fn drop(&mut self) {
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.pending = state.pending.saturating_sub(1);
        if state.pending == 0 {
            self.shared.idle.notify_waiters();
        }
    }
}
