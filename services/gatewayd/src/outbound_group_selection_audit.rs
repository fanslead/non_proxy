use std::{collections::HashMap, sync::Arc};

use nonproxy_model::{OutboundGroupId, OutboundGroupSpec, OutboundId};
use nonproxy_storage::{OutboundGroupSelectionAudit, OutboundGroupSelectionReason};
use tokio::sync::{Mutex, RwLock};

use crate::{Gateway, GatewayError, clock::unix_time_ms};

#[derive(Clone, Default)]
pub(crate) struct OutboundGroupSelectionTracker {
    selected: Arc<RwLock<HashMap<OutboundGroupId, TrackedSelection>>>,
    mutation: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct TrackedSelection {
    group_revision: u64,
    outbound_id: OutboundId,
}

impl OutboundGroupSelectionTracker {
    pub(crate) async fn record(
        &self,
        gateway: &Gateway,
        snapshot_version: u64,
        group: &OutboundGroupSpec,
        selected_outbound_id: &OutboundId,
    ) -> Result<(), GatewayError> {
        if self.matches(group, selected_outbound_id).await {
            return Ok(());
        }
        let _mutation = self.mutation.lock().await;
        let previous = self.selected.read().await.get(group.id()).cloned();
        if previous
            .as_ref()
            .is_some_and(|value| value.outbound_id == *selected_outbound_id)
        {
            self.store(group, selected_outbound_id).await;
            return Ok(());
        }
        let reason = match previous.as_ref() {
            None => OutboundGroupSelectionReason::InitialStableMember,
            Some(value) if value.group_revision == group.revision() => {
                OutboundGroupSelectionReason::StableHealthChanged
            }
            Some(_) => OutboundGroupSelectionReason::GroupRevisionChanged,
        };
        let mut event_nonce = [0_u8; 16];
        getrandom::fill(&mut event_nonce)
            .map_err(|error| GatewayError::Random(error.to_string()))?;
        let audit = OutboundGroupSelectionAudit {
            group_id: group.id().clone(),
            group_revision: group.revision(),
            previous_outbound_id: previous.as_ref().map(|value| value.outbound_id.clone()),
            selected_outbound_id: selected_outbound_id.clone(),
            snapshot_version,
            reason,
            occurred_at_unix_ms: unix_time_ms()?,
            event_nonce,
        };
        gateway
            .database
            .run(move |database| {
                database
                    .runtime_audit()
                    .record_outbound_group_selection(&audit)?;
                Ok(())
            })
            .await?;
        self.store(group, selected_outbound_id).await;
        Ok(())
    }

    pub(crate) async fn forget(&self, group_id: &OutboundGroupId) {
        let _mutation = self.mutation.lock().await;
        self.selected.write().await.remove(group_id);
    }

    async fn matches(&self, group: &OutboundGroupSpec, outbound_id: &OutboundId) -> bool {
        self.selected
            .read()
            .await
            .get(group.id())
            .is_some_and(|value| {
                value.group_revision == group.revision() && value.outbound_id == *outbound_id
            })
    }

    async fn store(&self, group: &OutboundGroupSpec, outbound_id: &OutboundId) {
        self.selected.write().await.insert(
            group.id().clone(),
            TrackedSelection {
                group_revision: group.revision(),
                outbound_id: outbound_id.clone(),
            },
        );
    }
}
