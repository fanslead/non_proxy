use nonproxy_model::{DecisionSpec, FailureMode, RouteAction};
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::{
    DefaultRoute, OutboundKind, OutboundReference, RoutingSettings, StorageError,
};

use crate::{
    Gateway, GatewayError, PublishedSnapshot,
    clock::unix_time_ms,
    snapshot_builder::{SnapshotBuildIdentity, build_snapshot, rebuild_snapshot},
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
        self.require_verified_default_proxy(&route, expected_revision, now)
            .await?;
        let capabilities = self.capabilities().clone();
        let system_policy_config = self.system_policy_config.clone();
        self.database
            .run(move |database| {
                let policies = database.policies().list()?;
                let outbounds = database.outbounds().list()?;
                let network_profiles = database.network_profiles().list()?;
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let snapshot_version = next_version(current)?;
                let default_decision = decision_for_route(&route)?;
                let published = build_snapshot(
                    capabilities,
                    &policies,
                    &outbounds,
                    &network_profiles,
                    default_decision,
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

    async fn require_verified_default_proxy(
        &self,
        route: &DefaultRoute,
        expected_revision: u64,
        now_unix_ms: u64,
    ) -> Result<(), GatewayError> {
        let DefaultRoute::Proxy(outbound_id) = route else {
            return Ok(());
        };
        let outbound_id = outbound_id.clone();
        let outbound = self
            .database
            .run(move |database| {
                let routing = database.routing_settings().get()?;
                if expected_revision == 0 || routing.revision() != expected_revision {
                    return Err(StorageError::RoutingRevisionConflict.into());
                }
                Ok(database.outbounds().get(&outbound_id)?)
            })
            .await?;
        let Some(outbound) = outbound else {
            return Err(StorageError::DefaultOutboundUnavailable.into());
        };
        if !outbound.enabled() || outbound.kind() != OutboundKind::Socks5 {
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
                    decoded.default_decision,
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
    }
}

fn route_for_decision(decision: &DecisionSpec) -> Result<DefaultRoute, GatewayError> {
    match decision.action() {
        RouteAction::Direct => Ok(DefaultRoute::Direct),
        RouteAction::Proxy => decision
            .outbound_id()
            .cloned()
            .map(DefaultRoute::Proxy)
            .ok_or(GatewayError::InvalidContract("代理默认决策缺少出口标识")),
        RouteAction::Block => Err(GatewayError::InvalidContract("默认路由配置不支持阻断决策")),
    }
}

fn next_version(current: u64) -> Result<u64, GatewayError> {
    current
        .checked_add(1)
        .ok_or(GatewayError::SnapshotVersionExhausted)
}
