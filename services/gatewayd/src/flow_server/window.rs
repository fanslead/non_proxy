use tokio::sync::{Mutex, Notify};

use super::FlowServiceError;

const MAXIMUM_WINDOW_BYTES: u64 = 16 * 1024 * 1024;

pub struct FlowWindow {
    available: Mutex<u64>,
    changed: Notify,
}

impl FlowWindow {
    pub fn new(initial: u32) -> Result<Self, FlowServiceError> {
        if initial == 0 || u64::from(initial) > MAXIMUM_WINDOW_BYTES {
            return Err(FlowServiceError::InvalidWindow);
        }
        Ok(Self {
            available: Mutex::new(u64::from(initial)),
            changed: Notify::new(),
        })
    }

    pub async fn add(&self, bytes: u32) -> Result<(), FlowServiceError> {
        if bytes == 0 {
            return Err(FlowServiceError::InvalidWindow);
        }
        let mut available = self.available.lock().await;
        let updated = available
            .checked_add(u64::from(bytes))
            .ok_or(FlowServiceError::InvalidWindow)?;
        if updated > MAXIMUM_WINDOW_BYTES {
            return Err(FlowServiceError::InvalidWindow);
        }
        *available = updated;
        drop(available);
        self.changed.notify_waiters();
        Ok(())
    }

    pub async fn take_up_to(&self, maximum: usize) -> Result<usize, FlowServiceError> {
        if maximum == 0 {
            return Err(FlowServiceError::InvalidWindow);
        }
        loop {
            let notified = self.changed.notified();
            let mut available = self.available.lock().await;
            if *available > 0 {
                let taken = (*available).min(maximum as u64);
                *available -= taken;
                return usize::try_from(taken).map_err(|_| FlowServiceError::InvalidWindow);
            }
            drop(available);
            notified.await;
        }
    }

    pub async fn take_exact(&self, bytes: usize) -> Result<(), FlowServiceError> {
        if bytes == 0 || bytes as u64 > MAXIMUM_WINDOW_BYTES {
            return Err(FlowServiceError::InvalidWindow);
        }
        loop {
            let notified = self.changed.notified();
            let mut available = self.available.lock().await;
            if *available >= bytes as u64 {
                *available -= bytes as u64;
                return Ok(());
            }
            drop(available);
            notified.await;
        }
    }

    pub async fn refund(&self, bytes: usize) -> Result<(), FlowServiceError> {
        let bytes = u32::try_from(bytes).map_err(|_| FlowServiceError::InvalidWindow)?;
        self.add(bytes).await
    }
}
