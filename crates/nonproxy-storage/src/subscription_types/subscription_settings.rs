use super::SubscriptionSource;
use crate::StorageError;

impl SubscriptionSource {
    pub fn settings_updated(
        &self,
        display_name: impl Into<String>,
        enabled: bool,
        refresh_interval_seconds: u32,
        revision: u64,
        updated_at_unix_ms: u64,
    ) -> Result<Self, StorageError> {
        let next_refresh_at_unix_ms = if enabled && !self.enabled {
            updated_at_unix_ms
        } else if enabled {
            self.next_refresh_at_unix_ms.min(
                updated_at_unix_ms
                    .saturating_add(u64::from(refresh_interval_seconds).saturating_mul(1_000)),
            )
        } else {
            self.next_refresh_at_unix_ms
        };
        Self::from_parts(
            self.id.clone(),
            display_name.into(),
            self.endpoint_credential.clone(),
            enabled,
            refresh_interval_seconds,
            revision,
            self.content_generation,
            self.consecutive_failures,
            next_refresh_at_unix_ms,
            self.last_attempted_at_unix_ms,
            self.last_succeeded_at_unix_ms,
            self.last_error_code.clone(),
            self.content_hash,
            self.node_count,
        )
    }
}
