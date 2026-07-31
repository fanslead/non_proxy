use std::collections::{BTreeMap, BTreeSet};

use nonproxy_proto::{control::v1::DiagnosticRedactionLevel, events::v1::event_envelope};
use nonproxy_storage::{DefaultRoute, EvidenceLevel};
use serde::Serialize;

use crate::{
    Gateway, GatewayError,
    diagnostics_export::DiagnosticsWindow,
    diagnostics_labels::{
        action, capabilities, component, evidence, outbound_kind, policy_source, runtime_state,
        severity, transport,
    },
    diagnostics_redaction::DiagnosticRedactor,
};

const MAX_DECISIONS_EXAMINED: usize = 500;
const MAX_STANDARD_SAMPLES: usize = 50;
const MAX_RECENT_ERRORS: usize = 100;

pub(crate) struct DiagnosticBuild {
    pub document: DiagnosticDocument,
    pub connection_sample_count: usize,
    pub error_count: usize,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticDocument {
    schema_version: u32,
    diagnostic_id: String,
    generated_at_unix_ms: u64,
    redaction: RedactionSummary,
    time_range: TimeRangeSummary,
    runtime: RuntimeSummary,
    configuration_summary: ConfigurationSummary,
    component_states: BTreeMap<String, String>,
    network_and_route_summary: PathSummary,
    recent_errors: Vec<ErrorSummary>,
    connection_samples: Vec<ConnectionSample>,
}

#[derive(Serialize)]
struct RedactionSummary {
    level: &'static str,
    strategy: &'static str,
    credentials_included: bool,
    endpoints_included: bool,
    payloads_included: bool,
}

#[derive(Serialize)]
struct TimeRangeSummary {
    start_unix_ms: u64,
    end_unix_ms: u64,
}

#[derive(Serialize)]
struct RuntimeSummary {
    gateway_version: &'static str,
    build_id: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    capabilities: Vec<&'static str>,
    active_snapshot_version: Option<u64>,
    pending_snapshot_version: Option<u64>,
    data_plane_ready: bool,
    default_route: &'static str,
    routing_revision: u64,
    dropped_decision_events: u64,
    latest_event_sequence: u64,
}

#[derive(Serialize)]
struct ConfigurationSummary {
    policies_total: usize,
    policies_enabled: usize,
    policies_by_source: BTreeMap<&'static str, usize>,
    policies_by_action: BTreeMap<&'static str, usize>,
    outbounds_total: usize,
    outbounds_enabled: usize,
    outbounds_by_kind: BTreeMap<&'static str, usize>,
}

#[derive(Serialize)]
struct PathSummary {
    stored_decision_total: u64,
    records_examined: usize,
    examination_truncated: bool,
    records_in_time_range: usize,
    direct_path_count: usize,
    proxy_path_count: usize,
    decision_only_count: usize,
    exit_evidence_count: usize,
    fail_open_direct_count: usize,
    observed_direct_interfaces: Vec<String>,
    observed_proxy_outbounds: Vec<String>,
}

#[derive(Serialize)]
struct ErrorSummary {
    occurred_at_unix_ms: u64,
    code: String,
    source: &'static str,
    severity: &'static str,
    snapshot_version: u64,
}

#[derive(Serialize)]
struct ConnectionSample {
    occurred_at_unix_ms: u64,
    application: String,
    destination: String,
    port: u16,
    transport: &'static str,
    action: &'static str,
    evidence: &'static str,
    observed_path: String,
    error_code: Option<String>,
    snapshot_version: u64,
}

pub(crate) async fn build(
    gateway: &Gateway,
    diagnostic_id: String,
    generated_at_unix_ms: u64,
    window: DiagnosticsWindow,
    redaction_level: DiagnosticRedactionLevel,
    redaction_salt: [u8; 32],
) -> Result<DiagnosticBuild, GatewayError> {
    let status = gateway.status().await?;
    let policies = gateway.list_policies().await?;
    let outbounds = gateway.list_outbounds().await?;
    let (decisions, decision_total) = gateway
        .list_connection_decisions(MAX_DECISIONS_EXAMINED, 0)
        .await?;
    let (events, _receiver) = gateway.events().subscribe(0)?;
    let redactor = DiagnosticRedactor::new(redaction_salt);

    let filtered = decisions
        .iter()
        .filter(|record| window.contains(record.occurred_at_unix_ms()))
        .collect::<Vec<_>>();
    let standard = redaction_level == DiagnosticRedactionLevel::Standard;
    let connection_samples = if standard {
        filtered
            .iter()
            .take(MAX_STANDARD_SAMPLES)
            .map(|record| ConnectionSample {
                occurred_at_unix_ms: record.occurred_at_unix_ms(),
                application: redactor.pseudonym("app", record.app_stable_id()),
                destination: redactor.pseudonym("target", record.destination()),
                port: record.destination_port(),
                transport: transport(record.transport()),
                action: action(record.action()),
                evidence: evidence(record.evidence_level()),
                observed_path: observed_path(record, &redactor),
                error_code: record.error_code().map(str::to_owned),
                snapshot_version: record.snapshot_version(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let (recent_errors, error_count) = recent_errors(&filtered, &events, window);
    let component_states = component_states(&events);
    let path_summary = path_summary(
        &filtered,
        decisions.len(),
        decision_total,
        standard.then_some(&redactor),
    );
    let connection_sample_count = connection_samples.len();

    Ok(DiagnosticBuild {
        document: DiagnosticDocument {
            schema_version: 1,
            diagnostic_id,
            generated_at_unix_ms,
            redaction: RedactionSummary {
                level: if standard { "standard" } else { "strict" },
                strategy: if standard {
                    "应用、目标、接口和出口使用每次导出独立盐值的不可逆短标识"
                } else {
                    "不包含逐连接样本，只保留聚合计数和稳定错误码"
                },
                credentials_included: false,
                endpoints_included: false,
                payloads_included: false,
            },
            time_range: TimeRangeSummary {
                start_unix_ms: window.start_unix_ms,
                end_unix_ms: window.end_unix_ms,
            },
            runtime: RuntimeSummary {
                gateway_version: env!("CARGO_PKG_VERSION"),
                build_id: option_env!("NONPROXY_BUILD_ID").unwrap_or("development"),
                target_os: std::env::consts::OS,
                target_arch: std::env::consts::ARCH,
                capabilities: capabilities(gateway),
                active_snapshot_version: status
                    .active
                    .as_ref()
                    .map(|value| value.artifact().snapshot_version()),
                pending_snapshot_version: status
                    .pending
                    .as_ref()
                    .map(|value| value.artifact().snapshot_version()),
                data_plane_ready: status.data_plane_ready,
                default_route: match status.routing.route() {
                    DefaultRoute::Direct => "direct",
                    DefaultRoute::Proxy(_) => "proxy",
                },
                routing_revision: status.routing.revision(),
                dropped_decision_events: status.dropped_decision_events,
                latest_event_sequence: gateway.events().latest_sequence()?,
            },
            configuration_summary: configuration_summary(&policies, &outbounds),
            component_states,
            network_and_route_summary: path_summary,
            recent_errors,
            connection_samples,
        },
        connection_sample_count,
        error_count,
    })
}

fn configuration_summary(
    policies: &[nonproxy_model::Policy],
    outbounds: &[nonproxy_storage::OutboundReference],
) -> ConfigurationSummary {
    let mut policies_by_source = BTreeMap::new();
    let mut policies_by_action = BTreeMap::new();
    for policy in policies {
        *policies_by_source
            .entry(policy_source(policy.source_kind()))
            .or_insert(0) += 1;
        *policies_by_action
            .entry(action(policy.decision().action()))
            .or_insert(0) += 1;
    }
    let mut outbounds_by_kind = BTreeMap::new();
    for outbound in outbounds {
        *outbounds_by_kind
            .entry(outbound_kind(outbound.kind()))
            .or_insert(0) += 1;
    }
    ConfigurationSummary {
        policies_total: policies.len(),
        policies_enabled: policies.iter().filter(|value| value.enabled()).count(),
        policies_by_source,
        policies_by_action,
        outbounds_total: outbounds.len(),
        outbounds_enabled: outbounds.iter().filter(|value| value.enabled()).count(),
        outbounds_by_kind,
    }
}

fn path_summary(
    records: &[&nonproxy_storage::ConnectionDecisionRecord],
    examined: usize,
    total: u64,
    redactor: Option<&DiagnosticRedactor>,
) -> PathSummary {
    let mut interfaces = BTreeSet::new();
    let mut outbounds = BTreeSet::new();
    for record in records {
        if let (Some(value), Some(redactor)) = (record.interface_name(), redactor) {
            interfaces.insert(redactor.pseudonym("interface", value));
        }
        if let (Some(value), Some(redactor)) = (record.outbound_id(), redactor) {
            outbounds.insert(redactor.pseudonym("outbound", value));
        }
    }
    PathSummary {
        stored_decision_total: total,
        records_examined: examined,
        examination_truncated: usize::try_from(total).map_or(true, |value| value > examined),
        records_in_time_range: records.len(),
        direct_path_count: records
            .iter()
            .filter(|value| value.interface_name().is_some())
            .count(),
        proxy_path_count: records
            .iter()
            .filter(|value| value.outbound_id().is_some())
            .count(),
        decision_only_count: records
            .iter()
            .filter(|value| value.evidence_level() == EvidenceLevel::Decision)
            .count(),
        exit_evidence_count: records
            .iter()
            .filter(|value| value.evidence_level() == EvidenceLevel::Exit)
            .count(),
        fail_open_direct_count: records
            .iter()
            .filter(|value| value.fail_open_direct())
            .count(),
        observed_direct_interfaces: interfaces.into_iter().collect(),
        observed_proxy_outbounds: outbounds.into_iter().collect(),
    }
}

fn recent_errors(
    records: &[&nonproxy_storage::ConnectionDecisionRecord],
    events: &[nonproxy_proto::events::v1::EventEnvelope],
    window: DiagnosticsWindow,
) -> (Vec<ErrorSummary>, usize) {
    let mut errors = records
        .iter()
        .filter_map(|record| {
            record.error_code().map(|code| ErrorSummary {
                occurred_at_unix_ms: record.occurred_at_unix_ms(),
                code: code.to_owned(),
                source: "connection_decision",
                severity: "error",
                snapshot_version: record.snapshot_version(),
            })
        })
        .collect::<Vec<_>>();
    errors.extend(events.iter().filter_map(|event| {
        let occurred_at = event.occurred_at.as_ref().and_then(timestamp_unix_ms)?;
        if event.error_code.is_empty() || !window.contains(occurred_at) {
            return None;
        }
        Some(ErrorSummary {
            occurred_at_unix_ms: occurred_at,
            code: event.error_code.clone(),
            source: "runtime_event",
            severity: severity(event.severity),
            snapshot_version: event.snapshot_version,
        })
    }));
    errors.sort_by_key(|value| std::cmp::Reverse(value.occurred_at_unix_ms));
    let count = errors.len();
    errors.truncate(MAX_RECENT_ERRORS);
    (errors, count)
}

fn component_states(
    events: &[nonproxy_proto::events::v1::EventEnvelope],
) -> BTreeMap<String, String> {
    let mut states = BTreeMap::from([("gateway".to_owned(), "ready".to_owned())]);
    for event in events {
        if let Some(event_envelope::Payload::ComponentHealthChanged(health)) = &event.payload {
            states.insert(
                component(health.component).to_owned(),
                runtime_state(health.state).to_owned(),
            );
        }
    }
    states
}

fn observed_path(
    record: &nonproxy_storage::ConnectionDecisionRecord,
    redactor: &DiagnosticRedactor,
) -> String {
    if let Some(value) = record.interface_name() {
        return redactor.pseudonym("interface", value);
    }
    if let Some(value) = record.outbound_id() {
        return redactor.pseudonym("outbound", value);
    }
    "decision-only".to_owned()
}

fn timestamp_unix_ms(value: &prost_types::Timestamp) -> Option<u64> {
    let seconds = u64::try_from(value.seconds).ok()?;
    let nanos = u32::try_from(value.nanos).ok()?;
    (nanos < 1_000_000_000).then(|| {
        seconds
            .saturating_mul(1_000)
            .saturating_add(u64::from(nanos / 1_000_000))
    })
}
