use nonproxy_model::{OutboundGroupId, OutboundId};
use nonproxy_proto::events::v1::RuntimeState;

use crate::{Gateway, clock::unix_time_ms, flow_server::FlowServiceError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OutboundGroupSelection {
    outbound_id: OutboundId,
}

impl OutboundGroupSelection {
    #[must_use]
    pub(crate) const fn outbound_id(&self) -> &OutboundId {
        &self.outbound_id
    }
}

impl Gateway {
    pub(crate) async fn select_outbound_group(
        &self,
        snapshot_version: u64,
        group_id: &OutboundGroupId,
    ) -> Result<OutboundGroupSelection, FlowServiceError> {
        if self.active_snapshot_version().await? != Some(snapshot_version) {
            return Err(FlowServiceError::PolicySnapshotUnavailable);
        }
        let snapshots = self.load_decision_snapshots(vec![snapshot_version]).await?;
        let snapshot = snapshots
            .get(&snapshot_version)
            .ok_or(FlowServiceError::PolicySnapshotUnavailable)?;
        let group = snapshot
            .outbound_groups()
            .get(group_id)
            .ok_or(FlowServiceError::OutboundGroupNotFound)?;
        let members = group.members().to_vec();
        let outbounds =
            self.database
                .run(move |database| {
                    let active_matches = database.snapshots().active()?.is_some_and(|record| {
                        record.artifact().snapshot_version() == snapshot_version
                    });
                    if !active_matches {
                        return Ok(None);
                    }
                    let outbounds = members
                        .into_iter()
                        .map(|id| Ok((id.clone(), database.outbounds().get(&id)?)))
                        .collect::<Result<Vec<_>, crate::GatewayError>>()?;
                    Ok(Some(outbounds))
                })
                .await?
                .ok_or(FlowServiceError::PolicySnapshotUnavailable)?;
        let now = unix_time_ms()?;
        for (id, outbound) in outbounds {
            let Some(outbound) = outbound.filter(|value| value.enabled()) else {
                continue;
            };
            if self
                .stable_outbound_health(&outbound, now)?
                .is_some_and(|health| health.state == RuntimeState::Ready)
            {
                return Ok(OutboundGroupSelection { outbound_id: id });
            }
        }
        Err(FlowServiceError::OutboundGroupUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_model::{OutboundGroupId, OutboundId};
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_proto::events::v1::RuntimeState;
    use nonproxy_storage::{
        OutboundGroup, OutboundGroupStrategy, OutboundKind, OutboundReference, PolicyDatabase,
        ProviderAck,
    };

    use crate::Gateway;

    #[tokio::test]
    async fn selects_first_stably_ready_member_from_the_active_snapshot() {
        let gateway = gateway_with_active_group().await;
        report(&gateway, "backup", RuntimeState::Ready);
        report(&gateway, "backup", RuntimeState::Ready);
        let group_id = group_id();

        let selected = gateway.select_outbound_group(1, &group_id).await;
        assert!(matches!(
            selected,
            Ok(value) if value.outbound_id().as_str() == "backup"
        ));

        report(&gateway, "primary", RuntimeState::Ready);
        assert!(matches!(
            gateway.select_outbound_group(1, &group_id).await,
            Ok(value) if value.outbound_id().as_str() == "backup"
        ));
        report(&gateway, "primary", RuntimeState::Ready);
        assert!(matches!(
            gateway.select_outbound_group(1, &group_id).await,
            Ok(value) if value.outbound_id().as_str() == "primary"
        ));

        gateway
            .save_outbound_group(
                OutboundGroup::new(
                    group_id.clone(),
                    "自动切换草稿",
                    OutboundGroupStrategy::Failover,
                    vec![outbound_id("backup"), outbound_id("primary")],
                    2,
                )
                .unwrap_or_else(|error| panic!("测试出口组草稿无效: {error}")),
                Some(1),
            )
            .await
            .unwrap_or_else(|error| panic!("测试出口组草稿保存失败: {error}"));
        assert!(matches!(
            gateway.select_outbound_group(1, &group_id).await,
            Ok(value) if value.outbound_id().as_str() == "primary"
        ));

        report(&gateway, "primary", RuntimeState::Failed);
        assert!(matches!(
            gateway.select_outbound_group(1, &group_id).await,
            Ok(value) if value.outbound_id().as_str() == "primary"
        ));
        report(&gateway, "primary", RuntimeState::Failed);
        assert!(matches!(
            gateway.select_outbound_group(1, &group_id).await,
            Ok(value) if value.outbound_id().as_str() == "backup"
        ));
    }

    #[tokio::test]
    async fn fails_closed_until_health_is_stable_and_rejects_a_stale_snapshot() {
        let gateway = gateway_with_active_group().await;
        report(&gateway, "primary", RuntimeState::Ready);

        assert!(matches!(
            gateway.select_outbound_group(1, &group_id()).await,
            Err(crate::flow_server::FlowServiceError::OutboundGroupUnavailable)
        ));
        assert!(matches!(
            gateway.select_outbound_group(2, &group_id()).await,
            Err(crate::flow_server::FlowServiceError::PolicySnapshotUnavailable)
        ));
    }

    async fn gateway_with_active_group() -> Gateway {
        let database = PolicyDatabase::open_in_memory(1)
            .unwrap_or_else(|error| panic!("出口组选择测试数据库打开失败: {error}"));
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let outbounds = ["primary", "backup"]
            .into_iter()
            .map(|id| {
                let outbound_id =
                    OutboundId::new(id).unwrap_or_else(|error| panic!("测试出口 ID 无效: {error}"));
                OutboundReference::new(
                    outbound_id,
                    OutboundKind::HttpConnect,
                    Some("127.0.0.1"),
                    Some(8_080),
                    None,
                    1,
                )
                .map(|value| (value, None))
                .unwrap_or_else(|error| panic!("测试出口无效: {error}"))
            })
            .collect();
        gateway
            .save_outbounds(outbounds)
            .await
            .unwrap_or_else(|error| panic!("测试出口保存失败: {error}"));
        gateway
            .save_outbound_group(
                OutboundGroup::new(
                    group_id(),
                    "自动切换",
                    OutboundGroupStrategy::Failover,
                    vec![outbound_id("primary"), outbound_id("backup")],
                    1,
                )
                .unwrap_or_else(|error| panic!("测试出口组无效: {error}")),
                None,
            )
            .await
            .unwrap_or_else(|error| panic!("测试出口组保存失败: {error}"));
        let published = gateway
            .compile_and_stage()
            .await
            .unwrap_or_else(|error| panic!("测试出口组快照编译失败: {error}"));
        let ack = ProviderAck::loaded(
            "transparent-proxy",
            1,
            *published.artifact().content_hash(),
            2,
        )
        .unwrap_or_else(|error| panic!("测试快照 ACK 无效: {error}"));
        gateway
            .acknowledge_provider_snapshot(1, ack, vec!["transparent-proxy".to_owned()])
            .await
            .unwrap_or_else(|error| panic!("测试出口组快照激活失败: {error}"));
        gateway
    }

    fn report(gateway: &Gateway, id: &str, state: RuntimeState) {
        let now = crate::clock::unix_time_ms()
            .unwrap_or_else(|error| panic!("测试出口健康时间读取失败: {error}"));
        gateway
            .report_outbound_health(outbound_id(id), 1, state, Some(10), now)
            .unwrap_or_else(|error| panic!("测试出口健康状态写入失败: {error}"));
    }

    fn group_id() -> OutboundGroupId {
        OutboundGroupId::new("automatic")
            .unwrap_or_else(|error| panic!("测试出口组 ID 无效: {error}"))
    }

    fn outbound_id(value: &str) -> OutboundId {
        OutboundId::new(value).unwrap_or_else(|error| panic!("测试出口 ID 无效: {error}"))
    }
}
