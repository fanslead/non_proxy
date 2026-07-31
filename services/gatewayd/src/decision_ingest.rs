use std::{collections::BTreeMap, net::IpAddr, sync::Arc};

use nonproxy_model::{
    AppIdentity, ConnectionContext, Destination, IpFamily, NetworkProfileId, OutboundId, Platform,
};
use nonproxy_policy::{CompiledPolicySnapshot, PolicyEngine};
use nonproxy_policy_compiler::PolicyCompiler;
use nonproxy_proto::{
    common::v1 as common_proto, provider::v1::DecisionRecord as ProtoDecisionRecord,
};
use nonproxy_storage::{ConnectionDecisionInput, DecisionEvidence, EvidenceLevel, SnapshotStatus};

use crate::{
    Gateway, GatewayError,
    clock::{micros_from_duration, unix_ms_from_timestamp, unix_time_ms},
    proto_policy::{decision_from_proto, transport_from_proto},
    snapshot_payload,
};

const MAX_DECISIONS_PER_BATCH: usize = 1_000;
const MAX_SNAPSHOTS_PER_BATCH: usize = 8;
const MAX_PATH_FIELD_LENGTH: usize = 128;
const MAX_DECISION_LATENCY_MICROS: u64 = 60_000_000;
const MAX_FUTURE_CLOCK_SKEW_MS: u64 = 5 * 60 * 1_000;
impl Gateway {
    pub(crate) async fn store_connection_decisions(
        &self,
        inputs: Vec<ConnectionDecisionInput>,
    ) -> Result<(), GatewayError> {
        self.database
            .run(move |database| {
                database.connection_decisions().save_batch(&inputs)?;
                Ok(())
            })
            .await
    }

    pub(crate) async fn ingest_decision_batch(
        &self,
        provider_id: String,
        provider_generation: u64,
        records: Vec<ProtoDecisionRecord>,
    ) -> Result<u32, GatewayError> {
        if records.is_empty() || records.len() > MAX_DECISIONS_PER_BATCH {
            return Err(GatewayError::InvalidRequest(
                "决策批次必须包含 1 到 1000 条记录",
            ));
        }
        let mut versions = records
            .iter()
            .map(reported_snapshot_version)
            .collect::<Result<Vec<_>, _>>()?;
        versions.sort_unstable();
        versions.dedup();
        if versions.len() > MAX_SNAPSHOTS_PER_BATCH {
            return Err(GatewayError::InvalidRequest(
                "单个决策批次最多引用 8 个策略快照",
            ));
        }
        let snapshots = self.load_decision_snapshots(versions).await?;
        let expected_platform = platform_for_provider(&provider_id)?;
        let now_unix_ms = unix_time_ms()?;
        let inputs = records
            .into_iter()
            .map(|record| {
                input_from_proto(
                    &provider_id,
                    provider_generation,
                    expected_platform,
                    now_unix_ms,
                    record,
                    &snapshots,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let accepted = inputs.len();
        self.store_connection_decisions(inputs).await?;
        u32::try_from(accepted)
            .map_err(|_| GatewayError::InvalidContract("决策接收数量超出协议范围"))
    }

    async fn load_decision_snapshots(
        &self,
        versions: Vec<u64>,
    ) -> Result<BTreeMap<u64, Arc<CompiledPolicySnapshot>>, GatewayError> {
        let lookup = self.decision_snapshots.lookup(&versions)?;
        let mut snapshots = lookup.found;
        if lookup.missing.is_empty() {
            return Ok(snapshots);
        }
        let missing = lookup.missing;
        let loaded = self
            .database
            .run(move |database| {
                let mut loaded = BTreeMap::new();
                for version in missing {
                    let record = database
                        .snapshots()
                        .get(version)?
                        .ok_or(GatewayError::InvalidRequest("决策引用的策略快照不存在"))?;
                    if record.status() == SnapshotStatus::Rejected {
                        return Err(GatewayError::InvalidRequest("决策不能引用已拒绝的策略快照"));
                    }
                    let artifact = record.artifact();
                    let decoded = snapshot_payload::decode_versioned(artifact.payload())?;
                    let compiled = PolicyCompiler::compile(decoded.into_compile_request(
                        artifact.snapshot_version(),
                        artifact.created_at_unix_ms(),
                    ))?;
                    if compiled.metadata().content_hash() != artifact.content_hash() {
                        return Err(GatewayError::InvalidContract(
                            "决策引用的策略快照内容哈希不一致",
                        ));
                    }
                    loaded.insert(version, Arc::new(compiled));
                }
                Ok(loaded)
            })
            .await?;
        self.decision_snapshots.insert(&loaded)?;
        snapshots.extend(loaded);
        Ok(snapshots)
    }
}

fn input_from_proto(
    provider_id: &str,
    provider_generation: u64,
    expected_platform: Platform,
    now_unix_ms: u64,
    record: ProtoDecisionRecord,
    snapshots: &BTreeMap<u64, Arc<CompiledPolicySnapshot>>,
) -> Result<ConnectionDecisionInput, GatewayError> {
    let reported = record
        .decision
        .ok_or(GatewayError::InvalidRequest("决策记录缺少判定结果"))?;
    let snapshot = snapshots
        .get(&reported.snapshot_version)
        .ok_or(GatewayError::InvalidRequest("决策引用的策略快照不存在"))?;
    let context = record
        .context
        .ok_or(GatewayError::InvalidRequest("决策记录缺少连接上下文"))?;
    let flow_id = context.flow_id;
    let occurred_at_unix_ms = unix_ms_from_timestamp(
        context
            .observed_at
            .as_ref()
            .ok_or(GatewayError::InvalidRequest("决策记录缺少观测时间"))?,
    )?;
    if occurred_at_unix_ms
        > now_unix_ms
            .checked_add(MAX_FUTURE_CLOCK_SKEW_MS)
            .ok_or(GatewayError::ClockOverflow)?
    {
        return Err(GatewayError::InvalidRequest(
            "决策观测时间超出允许的时钟偏差",
        ));
    }
    let app = app_from_proto(
        context
            .app
            .ok_or(GatewayError::InvalidRequest("决策记录缺少应用身份"))?,
        expected_platform,
    )?;
    let destination = destination_from_proto(
        context
            .destination
            .ok_or(GatewayError::InvalidRequest("决策记录缺少目标地址"))?,
    )?;
    let mut policy_context = ConnectionContext::new(app.clone(), destination.clone());
    if !context.network_profile_id.is_empty() {
        policy_context =
            policy_context.with_network_profile(NetworkProfileId::new(context.network_profile_id)?);
    }
    let expected = PolicyEngine::decide(snapshot, &policy_context);
    validate_reported_decision(&reported, &expected)?;
    let evidence = evidence_from_proto(
        record
            .evidence
            .ok_or(GatewayError::InvalidRequest("决策记录缺少证据等级"))?,
    )?;
    let latency = record
        .decision_latency
        .as_ref()
        .map(micros_from_duration)
        .transpose()?;
    if latency.is_some_and(|value| value > MAX_DECISION_LATENCY_MICROS) {
        return Err(GatewayError::InvalidRequest("策略判定耗时超过 60 秒"));
    }
    let error_code = record
        .error
        .map(|error| validate_error_code(error.code))
        .transpose()?;
    ConnectionDecisionInput::new(
        provider_id,
        provider_generation,
        flow_id,
        occurred_at_unix_ms,
        app,
        destination,
        expected,
        evidence,
        latency,
        error_code,
    )
    .map_err(GatewayError::from)
}

fn reported_snapshot_version(record: &ProtoDecisionRecord) -> Result<u64, GatewayError> {
    let version = record
        .decision
        .as_ref()
        .ok_or(GatewayError::InvalidRequest("决策记录缺少判定结果"))?
        .snapshot_version;
    if version == 0 {
        return Err(GatewayError::InvalidRequest("决策快照版本必须大于零"));
    }
    Ok(version)
}

fn validate_reported_decision(
    reported: &nonproxy_proto::policy::v1::Decision,
    expected: &nonproxy_model::Decision,
) -> Result<(), GatewayError> {
    let result = decision_from_proto(
        reported
            .result
            .clone()
            .ok_or(GatewayError::InvalidRequest("判定结果缺少路由动作"))?,
    )?;
    let expected_policy = expected.matched_policy_id().map(|value| value.as_str());
    let expected_rule = expected.matched_rule_id().map(|value| value.as_str());
    if &result != expected.result()
        || reported.snapshot_version != expected.snapshot_version()
        || optional_text(&reported.matched_policy_id) != expected_policy
        || optional_text(&reported.matched_rule_id) != expected_rule
        || reported.reason_code != expected.reason_code()
    {
        return Err(GatewayError::InvalidRequest(
            "Provider 上报判定与权威策略快照不一致",
        ));
    }
    Ok(())
}

fn evidence_from_proto(
    value: nonproxy_proto::provider::v1::DecisionEvidence,
) -> Result<DecisionEvidence, GatewayError> {
    let level = match common_proto::EvidenceLevel::try_from(value.level) {
        Ok(common_proto::EvidenceLevel::Decision) => EvidenceLevel::Decision,
        Ok(common_proto::EvidenceLevel::Path) => EvidenceLevel::Path,
        Ok(common_proto::EvidenceLevel::Exit) => {
            return Err(GatewayError::InvalidRequest(
                "Provider 不能直接声明出口探针证据",
            ));
        }
        _ => return Err(GatewayError::InvalidRequest("连接证据等级无效")),
    };
    DecisionEvidence::new(
        level,
        optional_owned(value.interface_name),
        optional_owned(value.outbound_id)
            .map(OutboundId::new)
            .transpose()?,
        optional_owned(value.exit_probe_id),
        value.fail_open_direct,
    )
    .map_err(GatewayError::from)
}

fn app_from_proto(
    value: common_proto::AppIdentity,
    expected_platform: Platform,
) -> Result<AppIdentity, GatewayError> {
    let platform = platform_from_proto(value.platform)?;
    if platform != expected_platform {
        return Err(GatewayError::InvalidRequest(
            "应用平台与 Provider 类型不一致",
        ));
    }
    let mut app = AppIdentity::new(platform, value.stable_id)?;
    if let Some(signer) = optional_owned(value.signer_id) {
        app = app.with_signer_id(signer)?;
    }
    if !value.executable_hash.is_empty() {
        app = app.with_executable_hash(value.executable_hash)?;
    }
    if let Some(path) = optional_owned(value.executable_path_hint) {
        app = app.with_path_hint(path)?;
    }
    if let Some(name) = optional_owned(value.display_name) {
        app = app.with_display_name(name)?;
    }
    if let Some(parent) = optional_owned(value.parent_stable_id) {
        app = app.with_parent_stable_id(parent)?;
    }
    if let Some(group) = optional_owned(value.helper_group_id) {
        app = app.with_helper_group_id(group)?;
    }
    Ok(app)
}

fn destination_from_proto(value: common_proto::Destination) -> Result<Destination, GatewayError> {
    let transport = transport_from_proto(value.transport)?;
    let hostname = optional_owned(value.hostname);
    let normalized = optional_owned(value.normalized_domain);
    let domain_input = normalized.as_deref().or(hostname.as_deref());
    let ip = optional_owned(value.ip_address)
        .map(|text| {
            text.parse::<IpAddr>()
                .map_err(|_| GatewayError::InvalidRequest("目标 IP 地址无效"))
        })
        .transpose()?;
    validate_ip_family(value.ip_family, ip)?;
    let port =
        u16::try_from(value.port).map_err(|_| GatewayError::InvalidRequest("目标端口无效"))?;
    let mut destination = Destination::new(domain_input, ip, port, transport)?;
    if let Some(normalized) = normalized.as_deref()
        && destination.domain().map(|domain| domain.as_ascii()) != Some(normalized)
    {
        return Err(GatewayError::InvalidRequest("目标规范化域名不一致"));
    }
    if let (Some(hostname), Some(domain)) = (hostname.as_deref(), destination.domain()) {
        let source = nonproxy_model::DomainName::normalize(hostname)?;
        if source.as_ascii() != domain.as_ascii() {
            return Err(GatewayError::InvalidRequest("目标主机名与规范化域名不一致"));
        }
    }
    if !value.interface_name.is_empty() {
        validate_path_field(&value.interface_name)?;
        destination = destination.with_interface_name(value.interface_name);
    }
    Ok(destination)
}

fn validate_ip_family(value: i32, ip: Option<IpAddr>) -> Result<(), GatewayError> {
    let reported = match common_proto::IpFamily::try_from(value) {
        Ok(common_proto::IpFamily::Unspecified) => None,
        Ok(common_proto::IpFamily::Ipv4) => Some(IpFamily::Ipv4),
        Ok(common_proto::IpFamily::Ipv6) => Some(IpFamily::Ipv6),
        Err(_) => return Err(GatewayError::InvalidRequest("目标 IP 地址族无效")),
    };
    let actual = ip.map(|value| match value {
        IpAddr::V4(_) => IpFamily::Ipv4,
        IpAddr::V6(_) => IpFamily::Ipv6,
    });
    if reported.is_some() && reported != actual {
        return Err(GatewayError::InvalidRequest("目标 IP 地址族不一致"));
    }
    Ok(())
}

fn platform_for_provider(provider_id: &str) -> Result<Platform, GatewayError> {
    match provider_id {
        "transparent-proxy" | "dns-proxy" => Ok(Platform::MacOs),
        "windows-wfp" | "windows-dns" => Ok(Platform::Windows),
        _ => Err(GatewayError::InvalidRequest(
            "Provider 类型无法产生连接决策",
        )),
    }
}

fn platform_from_proto(value: i32) -> Result<Platform, GatewayError> {
    match common_proto::Platform::try_from(value) {
        Ok(common_proto::Platform::Macos) => Ok(Platform::MacOs),
        Ok(common_proto::Platform::Windows) => Ok(Platform::Windows),
        _ => Err(GatewayError::InvalidRequest("应用平台无效")),
    }
}

fn validate_error_code(value: String) -> Result<String, GatewayError> {
    if !value.starts_with("NP_")
        || value.len() > MAX_PATH_FIELD_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(GatewayError::InvalidRequest("连接错误码格式无效"));
    }
    Ok(value)
}

fn validate_path_field(value: &str) -> Result<(), GatewayError> {
    if value.len() > MAX_PATH_FIELD_LENGTH
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(GatewayError::InvalidRequest("接口名称无效"));
    }
    Ok(())
}

fn optional_owned(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn optional_text(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests;
