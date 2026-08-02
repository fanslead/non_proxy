use nonproxy_model::{DecisionSpec, FailureMode, RouteAction};
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{DefaultRoute, OutboundReference, RoutingSettings, StorageError};

use crate::{
    Gateway, GatewayError, PublishedSnapshot,
    clock::unix_time_ms,
    snapshot_builder::{
        SnapshotBuildIdentity, SnapshotCatalog, SnapshotRoutingState, build_snapshot,
        rebuild_snapshot,
    },
    snapshot_payload,
};

#[derive(Clone, Debug)]
pub struct StagedRoutingSettings {
    settings: RoutingSettings,
    snapshot: PublishedSnapshot,
}

impl StagedRoutingSettings {
    #[must_use]
    pub const fn settings(&self) -> &RoutingSettings {
        &self.settings
    }

    #[must_use]
    pub const fn snapshot(&self) -> &PublishedSnapshot {
        &self.snapshot
    }
}

impl Gateway {
    pub async fn routing_settings(&self) -> Result<RoutingSettings, GatewayError> {
        self.database
            .run(|database| Ok(database.routing_settings().get()?))
            .await
    }

    pub async fn list_outbounds_with_routing(
        &self,
    ) -> Result<(Vec<OutboundReference>, RoutingSettings), GatewayError> {
        self.database
            .run(|database| {
                Ok((
                    database.outbounds().list()?,
                    database.routing_settings().get()?,
                ))
            })
            .await
    }

    pub async fn set_default_route_and_stage(
        &self,
        route: DefaultRoute,
        expected_revision: u64,
    ) -> Result<StagedRoutingSettings, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.require_verified_default_target(&route, expected_revision, now)
            .await?;
        let capabilities = self.capabilities().clone();
        let system_policy_config = self.system_policy_config.clone();
        self.database
            .run(move |database| {
                let policies = database.policies().list()?;
                let outbounds = database.outbounds().list()?;
                let outbound_groups = database.outbound_groups().list()?;
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
                let snapshot_version = next_version(current)?;
                let default_decision = decision_for_route(&route)?;
                let published = build_snapshot(
                    capabilities,
                    SnapshotCatalog::new(
                        &policies,
                        &outbounds,
                        &outbound_groups,
                        &network_profiles,
                    ),
                    SnapshotRoutingState::new(default_decision, runtime_override),
                    SnapshotBuildIdentity::new(snapshot_version, now),
                    &system_policy_config,
                )?;
                let settings = database.routing_settings().set_and_stage(
                    &route,
                    expected_revision,
                    published.artifact(),
                    now,
                )?;
                Ok(StagedRoutingSettings {
                    settings,
                    snapshot: published,
                })
            })
            .await
    }

    async fn require_verified_default_target(
        &self,
        route: &DefaultRoute,
        expected_revision: u64,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        match route {
            DefaultRoute::Direct => Ok(()),
            DefaultRoute::Proxy(outbound_id) => {
                let outbound = self
                    .load_default_outbounds(expected_revision, None, Some(outbound_id.clone()))
                    .await?
                    .pop()
                    .ok_or(StorageError::DefaultOutboundUnavailable)?;
                if !outbound.enabled() || !outbound.kind().supports_default_route() {
                    return Err(StorageError::DefaultOutboundUnavailable.into());
                }
                if !matches!(
                    self.outbound_health(&outbound, now_unix_ms)?,
                    Some(observation) if observation.state == RuntimeState::Ready
                ) {
                    return Err(GatewayError::DefaultOutboundUnverified);
                }
                Ok(())
            }
            DefaultRoute::Group(group_id) => {
                let outbounds = self
                    .load_default_outbounds(expected_revision, Some(group_id.clone()), None)
                    .await?;
                if outbounds.len() < 2
                    || outbounds
                        .iter()
                        .any(|value| !value.enabled() || !value.kind().supports_default_route())
                {
                    return Err(StorageError::DefaultOutboundUnavailable.into());
                }
                for outbound in outbounds {
                    if self
                        .stable_outbound_health(&outbound, now_unix_ms)?
                        .is_some_and(|health| health.state == RuntimeState::Ready)
                    {
                        return Ok(());
                    }
                }
                Err(GatewayError::DefaultOutboundUnverified)
            }
        }
    }

    async fn load_default_outbounds(
        &self,
        expected_revision: u64,
        group_id: Option<nonproxy_model::OutboundGroupId>,
        outbound_id: Option<nonproxy_model::OutboundId>,
    ) -> Result<Vec<OutboundReference>, GatewayError> {
        self.database
            .run(move |database| {
                let routing = database.routing_settings().get()?;
                if expected_revision == 0 || routing.revision() != expected_revision {
                    return Err(StorageError::RoutingRevisionConflict.into());
                }
                let members = match (group_id, outbound_id) {
                    (Some(group_id), None) => database
                        .outbound_groups()
                        .get(&group_id)?
                        .map(|group| group.members().to_vec())
                        .ok_or(StorageError::DefaultOutboundUnavailable)?,
                    (None, Some(outbound_id)) => vec![outbound_id],
                    _ => return Err(StorageError::DefaultOutboundUnavailable.into()),
                };
                members
                    .into_iter()
                    .map(|id| {
                        database
                            .outbounds()
                            .get(&id)?
                            .ok_or(StorageError::DefaultOutboundUnavailable.into())
                    })
                    .collect()
            })
            .await
    }

    pub(crate) async fn stage_rollback_with_route(
        &self,
        target_snapshot_version: u64,
        expected_active_snapshot_version: u64,
    ) -> Result<PublishedSnapshot, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let system_policy_config = self.system_policy_config.clone();
        self.database
            .run(move |database| {
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let next = next_version(current)?;
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
                let source = database
                    .snapshots()
                    .get(target_snapshot_version)?
                    .ok_or(StorageError::SnapshotNotFound)?;
                let decoded = snapshot_payload::decode_versioned(source.artifact().payload())?;
                let route = route_for_decision(&decoded.default_decision)?;
                let revision = database.routing_settings().get()?.revision();
                let network_profiles = if decoded.includes_network_profiles {
                    decoded.network_profiles.clone()
                } else {
                    database
                        .network_profiles()
                        .list()?
                        .iter()
                        .map(nonproxy_storage::NetworkProfileReference::binding)
                        .collect()
                };
                let published = rebuild_snapshot(
                    decoded.capabilities,
                    &decoded.policies,
                    &network_profiles,
                    SnapshotRoutingState::new(decoded.default_decision, runtime_override),
                    SnapshotBuildIdentity::new(next, now),
                    &system_policy_config,
                )?;
                database.routing_settings().set_and_stage_rebuilt_rollback(
                    &route,
                    revision,
                    published.artifact(),
                    target_snapshot_version,
                    expected_active_snapshot_version,
                    now,
                )?;
                Ok(published)
            })
            .await
    }
}

pub(crate) fn decision_for_route(route: &DefaultRoute) -> Result<DecisionSpec, GatewayError> {
    match route {
        DefaultRoute::Direct => Ok(DecisionSpec::direct()),
        DefaultRoute::Proxy(outbound_id) => Ok(DecisionSpec::new(
            RouteAction::Proxy,
            Some(outbound_id.clone()),
            FailureMode::Closed,
        )?),
        DefaultRoute::Group(group_id) => Ok(DecisionSpec::proxy_group(
            group_id.clone(),
            FailureMode::Closed,
        )?),
    }
}

fn route_for_decision(decision: &DecisionSpec) -> Result<DefaultRoute, GatewayError> {
    match decision.action() {
        RouteAction::Direct => Ok(DefaultRoute::Direct),
        RouteAction::Proxy => match decision.proxy_target() {
            Some(nonproxy_model::ProxyTarget::Outbound(id)) => Ok(DefaultRoute::Proxy(id.clone())),
            Some(nonproxy_model::ProxyTarget::Group(id)) => Ok(DefaultRoute::Group(id.clone())),
            None => Err(GatewayError::InvalidContract("代理默认决策缺少出口目标")),
        },
        RouteAction::Block => Err(GatewayError::InvalidContract("默认路由配置不支持阻断决策")),
    }
}

pub(crate) fn next_version(current: u64) -> Result<u64, GatewayError> {
    current
        .checked_add(1)
        .ok_or(GatewayError::SnapshotVersionExhausted)
}
