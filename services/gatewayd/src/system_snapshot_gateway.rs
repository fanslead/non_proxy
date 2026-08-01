use nonproxy_storage::{
    ProviderAck, ProviderAckState, SnapshotRecord, SnapshotStatus, StorageError,
};

use crate::{
    Gateway, GatewayError,
    clock::unix_time_ms,
    snapshot_builder::{SnapshotBuildIdentity, SnapshotRoutingState, rebuild_snapshot},
    snapshot_payload, system_policies,
};

const SYSTEM_POLICY_UPGRADE_CODE: &str = "NP_SNAPSHOT_SYSTEM_POLICY_UPGRADE";

impl Gateway {
    pub(crate) async fn reconcile_required_system_snapshot(&self) -> Result<bool, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let system_policy_config = self.system_policy_config.clone();
        let (changed, active_ready) = self
            .database
            .run(move |database| {
                let pending = database.snapshots().pending()?;
                let active = database.snapshots().active()?;
                let active_ready = match active.as_ref() {
                    Some(record) => {
                        let (policies, _, _) =
                            snapshot_payload::decode(record.artifact().payload())?;
                        system_policies::contains_required(&policies, &system_policy_config)?
                    }
                    None => pending.is_none(),
                };
                let candidate = pending.as_ref().cloned().or(active);
                let Some(candidate) = candidate else {
                    return Ok((false, active_ready));
                };
                let decoded = snapshot_payload::decode_versioned(candidate.artifact().payload())?;
                if system_policies::contains_required(&decoded.policies, &system_policy_config)? {
                    return Ok((false, active_ready));
                }
                let current = database.snapshots().latest_version()?.unwrap_or(0);
                let snapshot_version = current
                    .checked_add(1)
                    .ok_or(GatewayError::SnapshotVersionExhausted)?;
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
                    SnapshotRoutingState::new(
                        decoded.default_decision,
                        decoded
                            .runtime_override
                            .filter(|value| value.is_active_at(now)),
                    ),
                    SnapshotBuildIdentity::new(snapshot_version, now),
                    &system_policy_config,
                )?;
                if pending.is_some() {
                    database.snapshots().replace_pending(
                        published.artifact(),
                        SYSTEM_POLICY_UPGRADE_CODE,
                        now,
                    )?;
                } else {
                    database.snapshots().stage(published.artifact())?;
                }
                Ok((true, active_ready))
            })
            .await?;
        self.set_system_snapshot_ready(active_ready);
        Ok(changed)
    }

    pub async fn acknowledge_provider_snapshot(
        &self,
        snapshot_version: u64,
        acknowledgement: ProviderAck,
        required_provider_ids: Vec<String>,
    ) -> Result<SnapshotRecord, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let record = self
            .database
            .run(move |database| {
                database
                    .snapshots()
                    .record_ack(snapshot_version, &acknowledgement)?;
                if acknowledgement.state() == ProviderAckState::Loaded {
                    match database.snapshots().activate(
                        snapshot_version,
                        &required_provider_ids,
                        now,
                    ) {
                        Ok(()) | Err(StorageError::ProviderAcknowledgementMissing) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                database
                    .snapshots()
                    .get(snapshot_version)?
                    .ok_or_else(|| StorageError::SnapshotNotFound.into())
            })
            .await?;
        if record.status() == SnapshotStatus::Active {
            let (policies, _, _) = snapshot_payload::decode(record.artifact().payload())?;
            self.set_system_snapshot_ready(system_policies::contains_required(
                &policies,
                &self.system_policy_config,
            )?);
        }
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_model::{
        DecisionSpec, NetworkFingerprint, NetworkFingerprintKind, NetworkMatcher, NetworkProfileId,
        NetworkProfileReference, Policy, PolicyId, PolicyMatch, PolicyMetadata, PolicyOrigin,
        PolicySourceKind,
    };
    use nonproxy_policy_compiler::{CompileCapabilities, CompileRequest, PolicyCompiler};
    use nonproxy_proto::events::v1::RuntimeState;
    use nonproxy_proto::policy::v1::CompiledPolicyPayload;
    use nonproxy_storage::{PolicyDatabase, ProviderAck, SnapshotArtifact, SnapshotStatus};
    use prost::Message;

    use super::SYSTEM_POLICY_UPGRADE_CODE;
    use crate::{
        Gateway, snapshot_payload,
        system_policies::{self, SystemPolicyConfig},
    };

    #[tokio::test]
    async fn startup_atomically_replaces_a_legacy_pending_snapshot() {
        let database = PolicyDatabase::open_in_memory(1_000);
        let Ok(mut database) = database else {
            panic!("测试数据库打开失败: {database:?}");
        };
        let legacy = legacy_artifact(1, 1_100);
        if let Err(error) = database.snapshots().stage(&legacy) {
            panic!("旧待发布快照暂存失败: {error}");
        }
        let config = signed_config();
        let gateway =
            Gateway::new_with_system_policy(database, CompileCapabilities::full(), config.clone());

        assert!(matches!(
            gateway.reconcile_required_system_snapshot().await,
            Ok(true)
        ));
        let state = gateway
            .database
            .run(|database| {
                Ok((
                    database.snapshots().get(1)?,
                    database.snapshots().pending()?,
                ))
            })
            .await;
        let Ok((Some(replaced), Some(pending))) = state else {
            panic!("升级后的快照状态读取失败: {state:?}");
        };
        let decoded = snapshot_payload::decode(pending.artifact().payload());
        let Ok((policies, _, _)) = decoded else {
            panic!("升级快照解码失败: {decoded:?}");
        };

        assert_eq!(replaced.status(), SnapshotStatus::Rejected);
        assert_eq!(replaced.failure_code(), Some(SYSTEM_POLICY_UPGRADE_CODE));
        assert_eq!(pending.artifact().snapshot_version(), 2);
        assert!(matches!(
            system_policies::contains_required(&policies, &config),
            Ok(true)
        ));
    }

    #[tokio::test]
    async fn startup_preserves_active_snapshot_until_upgraded_snapshot_is_acked() {
        let database = PolicyDatabase::open_in_memory(1_000);
        let Ok(mut database) = database else {
            panic!("测试数据库打开失败: {database:?}");
        };
        let legacy = legacy_artifact(1, 1_100);
        if let Err(error) = database.snapshots().stage(&legacy) {
            panic!("旧活动快照暂存失败: {error}");
        }
        let ack = ProviderAck::loaded("transparent-proxy", 1, *legacy.content_hash(), 1_200);
        let Ok(ack) = ack else {
            panic!("旧活动快照 ACK 创建失败: {ack:?}");
        };
        if let Err(error) = database.snapshots().record_ack(1, &ack) {
            panic!("旧活动快照 ACK 保存失败: {error}");
        }
        if let Err(error) =
            database
                .snapshots()
                .activate(1, &["transparent-proxy".to_owned()], 1_300)
        {
            panic!("旧活动快照激活失败: {error}");
        }
        let gateway =
            Gateway::new_with_system_policy(database, CompileCapabilities::full(), signed_config());

        assert!(matches!(
            gateway.reconcile_required_system_snapshot().await,
            Ok(true)
        ));
        for provider_id in ["transparent-proxy", "dns-proxy"] {
            assert!(
                gateway
                    .report_provider_health(provider_id, 1, RuntimeState::Ready, 1, u64::MAX,)
                    .is_ok()
            );
        }
        let status = gateway.status().await;
        let Ok(status) = status else {
            panic!("升级后的网关状态读取失败: {status:?}");
        };

        assert!(
            !status.data_plane_ready,
            "Provider 即使报告旧活动快照 Ready，系统状态也不能越过防回环门"
        );
        assert!(matches!(
            status.active,
            Some(record) if record.artifact().snapshot_version() == 1
        ));
        assert!(matches!(
            status.pending,
            Some(record) if record.artifact().snapshot_version() == 2
        ));
        assert!(
            !gateway.system_snapshot_ready(),
            "旧活动快照仍生效时必须阻止 gatewayd 建立代理上游连接"
        );

        let pending = gateway.provider_snapshot(0).await;
        let Ok(Some(pending)) = pending else {
            panic!("升级待发布快照读取失败: {pending:?}");
        };
        let acknowledgement = ProviderAck::loaded(
            "transparent-proxy",
            2,
            *pending.record().artifact().content_hash(),
            1_400,
        );
        let Ok(acknowledgement) = acknowledgement else {
            panic!("升级快照 ACK 创建失败: {acknowledgement:?}");
        };
        let activated = gateway
            .acknowledge_provider_snapshot(
                pending.record().artifact().snapshot_version(),
                acknowledgement,
                vec!["transparent-proxy".to_owned()],
            )
            .await;
        let Ok(activated) = activated else {
            panic!("升级快照激活失败: {activated:?}");
        };

        assert_eq!(activated.status(), SnapshotStatus::Active);
        assert!(
            gateway.system_snapshot_ready(),
            "当前防回环规则激活后应恢复 gatewayd 代理上游连接"
        );
    }

    #[tokio::test]
    async fn legacy_network_snapshot_upgrade_uses_current_private_binding() {
        let database = PolicyDatabase::open_in_memory(1_000);
        let Ok(mut database) = database else {
            panic!("旧网络快照升级测试数据库打开失败: {database:?}");
        };
        let profile = network_profile();
        database
            .network_profiles()
            .save(&profile, None, 1_050)
            .unwrap_or_else(|error| panic!("旧网络快照配置档保存失败: {error}"));
        let legacy = legacy_artifact_with_policies(1, 1_100, vec![network_policy(profile.id())]);
        database
            .snapshots()
            .stage(&legacy)
            .unwrap_or_else(|error| panic!("旧网络快照暂存失败: {error}"));
        let gateway =
            Gateway::new_with_system_policy(database, CompileCapabilities::full(), signed_config());

        assert!(matches!(
            gateway.reconcile_required_system_snapshot().await,
            Ok(true)
        ));
        let pending = gateway
            .database
            .run(|database| Ok(database.snapshots().pending()?))
            .await
            .unwrap_or_else(|error| panic!("升级网络快照读取失败: {error}"))
            .unwrap_or_else(|| panic!("升级网络快照必须创建待发布版本"));
        let decoded = snapshot_payload::decode_versioned(pending.artifact().payload())
            .unwrap_or_else(|error| panic!("升级网络快照解码失败: {error}"));

        assert!(decoded.includes_network_profiles);
        assert!(matches!(
            decoded.network_profiles.as_slice(),
            [binding] if binding.id() == profile.id()
                && binding.fingerprint() == profile.fingerprint()
        ));
    }

    fn signed_config() -> SystemPolicyConfig {
        match SystemPolicyConfig::new(Some("TEAM123456".to_owned())) {
            Ok(value) => value,
            Err(error) => panic!("签名系统策略配置无效: {error}"),
        }
    }

    fn legacy_artifact(snapshot_version: u64, created_at_unix_ms: u64) -> SnapshotArtifact {
        legacy_artifact_with_policies(snapshot_version, created_at_unix_ms, Vec::new())
    }

    fn legacy_artifact_with_policies(
        snapshot_version: u64,
        created_at_unix_ms: u64,
        policies: Vec<Policy>,
    ) -> SnapshotArtifact {
        let capabilities = CompileCapabilities::full();
        let default_decision = DecisionSpec::direct();
        let compiled = PolicyCompiler::compile(CompileRequest::new(
            snapshot_version,
            created_at_unix_ms,
            default_decision.clone(),
            policies.clone(),
            capabilities.clone(),
        ));
        let Ok(compiled) = compiled else {
            panic!("旧快照编译失败: {compiled:?}");
        };
        let current_payload =
            snapshot_payload::encode(&policies, &capabilities, &default_decision, &[], None);
        let Ok(current_payload) = current_payload else {
            panic!("旧快照编码前置失败: {current_payload:?}");
        };
        let payload = CompiledPolicyPayload::decode(current_payload.as_slice()).map(|mut value| {
            value.format_version = 1;
            value.network_profiles.clear();
            value.encode_to_vec()
        });
        let Ok(payload) = payload else {
            panic!("旧快照编码失败: {payload:?}");
        };
        let metadata = compiled.metadata();
        match SnapshotArtifact::new(
            snapshot_version,
            metadata.schema_version(),
            created_at_unix_ms,
            *metadata.content_hash(),
            metadata.policy_count(),
            payload,
        ) {
            Ok(value) => value,
            Err(error) => panic!("旧快照产物创建失败: {error}"),
        }
    }

    fn network_profile() -> NetworkProfileReference {
        NetworkProfileReference::new(
            NetworkProfileId::new("office")
                .unwrap_or_else(|error| panic!("旧快照网络标识创建失败: {error}")),
            "办公室",
            NetworkFingerprint::new(NetworkFingerprintKind::WifiSsidSha256, "a".repeat(64))
                .unwrap_or_else(|error| panic!("旧快照网络指纹创建失败: {error}")),
            1,
        )
        .unwrap_or_else(|error| panic!("旧快照网络配置档创建失败: {error}"))
    }

    fn network_policy(profile_id: &NetworkProfileId) -> Policy {
        Policy::new(
            PolicyId::new("office-policy")
                .unwrap_or_else(|error| panic!("旧快照网络策略标识创建失败: {error}")),
            "办公室直连",
            PolicyMatch::new(
                None,
                None,
                None,
                Some(NetworkMatcher::new(profile_id.clone())),
                Vec::new(),
                Vec::new(),
            )
            .unwrap_or_else(|error| panic!("旧快照网络匹配器创建失败: {error}")),
            DecisionSpec::direct(),
            PolicyMetadata::new(PolicySourceKind::Network, 0, PolicyOrigin::User, 1),
        )
        .unwrap_or_else(|error| panic!("旧快照网络策略创建失败: {error}"))
    }
}
