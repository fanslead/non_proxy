use nonproxy_model::OutboundGroupId;
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{DefaultRoute, OutboundGroup, OutboundReference, RoutingSettings};

use crate::{
    Gateway, GatewayError, PublishedSnapshot,
    clock::unix_time_ms,
    routing_gateway::decision_for_route,
    snapshot_builder::{
        SnapshotBuildIdentity, SnapshotCatalog, SnapshotRoutingState, build_snapshot,
    },
    snapshot_payload,
};

#[derive(Clone, Debug)]
pub struct SavedOutboundGroup {
    group: OutboundGroup,
    routing: RoutingSettings,
    snapshot: Option<PublishedSnapshot>,
}

impl SavedOutboundGroup {
    #[must_use]
    pub const fn group(&self) -> &OutboundGroup {
        &self.group
    }

    #[must_use]
    pub const fn routing(&self) -> &RoutingSettings {
        &self.routing
    }

    #[must_use]
    pub const fn snapshot(&self) -> Option<&PublishedSnapshot> {
        self.snapshot.as_ref()
    }
}

impl Gateway {
    pub async fn list_outbound_groups(&self) -> Result<Vec<OutboundGroup>, GatewayError> {
        self.database
            .run(|database| Ok(database.outbound_groups().list()?))
            .await
    }

    pub async fn list_outbound_groups_with_routing(
        &self,
    ) -> Result<(Vec<OutboundGroup>, RoutingSettings), GatewayError> {
        self.database
            .run(|database| {
                Ok((
                    database.outbound_groups().list()?,
                    database.routing_settings().get()?,
                ))
            })
            .await
    }

    pub async fn save_outbound_group(
        &self,
        group: OutboundGroup,
        expected_revision: Option<u64>,
    ) -> Result<SavedOutboundGroup, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let group_id = group.id().clone();
        let (routing, current_group) = self
            .database
            .run(move |database| {
                Ok((
                    database.routing_settings().get()?,
                    database.outbound_groups().get(&group_id)?,
                ))
            })
            .await?;
        let revision_matches = match (current_group.as_ref(), expected_revision) {
            (None, None) => group.revision() == 1,
            (Some(current), Some(expected)) => {
                current.revision() == expected && expected.checked_add(1) == Some(group.revision())
            }
            _ => false,
        };
        if !revision_matches {
            return Err(nonproxy_storage::StorageError::OutboundGroupRevisionConflict.into());
        }
        let is_default = matches!(
            routing.route(),
            DefaultRoute::Group(group_id) if group_id == group.id()
        );
        if is_default {
            let member_ids = group.members().to_vec();
            let outbounds = self
                .database
                .run(move |database| {
                    member_ids
                        .into_iter()
                        .map(|id| {
                            database.outbounds().get(&id)?.ok_or(
                                nonproxy_storage::StorageError::DefaultOutboundUnavailable.into(),
                            )
                        })
                        .collect::<Result<Vec<OutboundReference>, GatewayError>>()
                })
                .await?;
            if outbounds.len() < 2
                || outbounds
                    .iter()
                    .any(|value| !value.enabled() || !value.kind().supports_default_route())
            {
                return Err(nonproxy_storage::StorageError::DefaultOutboundUnavailable.into());
            }
            let mut has_ready_member = false;
            for outbound in &outbounds {
                if self
                    .stable_outbound_health(outbound, now)?
                    .is_some_and(|value| value.state == RuntimeState::Ready)
                {
                    has_ready_member = true;
                    break;
                }
            }
            if !has_ready_member {
                return Err(GatewayError::DefaultOutboundUnverified);
            }
        }
        let capabilities = self.capabilities().clone();
        let system_policy_config = self.system_policy_config.clone();
        self.database
            .run(move |database| {
                let routing = database.routing_settings().get()?;
                let snapshot = if is_default {
                    let policies = database.policies().list()?;
                    let outbounds = database.outbounds().list()?;
                    let mut groups = database.outbound_groups().list()?;
                    let stored = groups
                        .iter_mut()
                        .find(|value| value.id() == group.id())
                        .ok_or(nonproxy_storage::StorageError::OutboundGroupRevisionConflict)?;
                    *stored = group.clone();
                    let network_profiles = database.network_profiles().list()?;
                    let current = database.snapshots().latest_version()?.unwrap_or(0);
                    let runtime_override = database
                        .snapshots()
                        .active()?
                        .map(|record| {
                            snapshot_payload::effective_runtime_override(
                                record.artifact().payload(),
                                now,
                            )
                        })
                        .transpose()?
                        .flatten();
                    let published = build_snapshot(
                        capabilities,
                        SnapshotCatalog::new(&policies, &outbounds, &groups, &network_profiles),
                        SnapshotRoutingState::new(
                            decision_for_route(routing.route())?,
                            runtime_override,
                        ),
                        SnapshotBuildIdentity::new(
                            super::routing_gateway::next_version(current)?,
                            now,
                        ),
                        &system_policy_config,
                    )?;
                    database.outbound_groups().save_and_stage(
                        &group,
                        expected_revision,
                        published.artifact(),
                        now,
                    )?;
                    Some(published)
                } else {
                    database
                        .outbound_groups()
                        .save(&group, expected_revision, now)?;
                    None
                };
                Ok(SavedOutboundGroup {
                    group,
                    routing,
                    snapshot,
                })
            })
            .await
    }

    pub async fn delete_outbound_group(
        &self,
        group_id: OutboundGroupId,
        expected_revision: u64,
    ) -> Result<(), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let deleted_group_id = group_id.clone();
        self.database
            .run(move |database| {
                database
                    .outbound_groups()
                    .delete(&deleted_group_id, expected_revision, now)?;
                Ok(())
            })
            .await?;
        self.outbound_group_selections.forget(&group_id).await;
        Ok(())
    }
}
