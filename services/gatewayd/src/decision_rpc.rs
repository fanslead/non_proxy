use nonproxy_model::{FailureMode, Platform, RouteAction, Transport};
use nonproxy_proto::{
    common::v1::{self as common_proto, PageRequest, PageResponse},
    control::v1::{
        ConnectionDecisionSummary, ListConnectionDecisionsRequest, ListConnectionDecisionsResponse,
    },
};
use nonproxy_storage::{ConnectionDecisionRecord, EvidenceLevel};
use tonic::Status;

use crate::{Gateway, clock::timestamp_from_unix_ms, control_rpc_helpers::internal_status};

const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE_SIZE: usize = 200;

impl Gateway {
    pub(crate) async fn list_connection_decisions(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<ConnectionDecisionRecord>, u64), crate::GatewayError> {
        self.database
            .run(move |database| Ok(database.connection_decisions().list_recent(limit, offset)?))
            .await
    }
}

pub async fn list(
    gateway: &Gateway,
    request: ListConnectionDecisionsRequest,
) -> Result<ListConnectionDecisionsResponse, Status> {
    let (limit, offset) = parse_page(request.page)?;
    let (records, total) = gateway
        .list_connection_decisions(limit, offset)
        .await
        .map_err(internal_status)?;
    if u64::try_from(offset).map_or(true, |value| value > total) {
        return Err(Status::invalid_argument("page_token 超出结果范围"));
    }
    let consumed = offset.saturating_add(records.len());
    let next_page_token = if u64::try_from(consumed).is_ok_and(|value| value < total) {
        consumed.to_string()
    } else {
        String::new()
    };
    let decisions = records
        .iter()
        .map(to_proto)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ListConnectionDecisionsResponse {
        decisions,
        page: Some(PageResponse { next_page_token }),
        total_count: total,
    })
}

fn parse_page(page: Option<PageRequest>) -> Result<(usize, usize), Status> {
    let page = page.unwrap_or_default();
    let limit = match page.page_size {
        0 => DEFAULT_PAGE_SIZE,
        1..=200 => usize::try_from(page.page_size)
            .map_err(|_| Status::invalid_argument("page_size 无效"))?,
        _ => return Err(Status::invalid_argument("page_size 最大为 200")),
    };
    if limit > MAX_PAGE_SIZE {
        return Err(Status::invalid_argument("page_size 最大为 200"));
    }
    let offset = if page.page_token.is_empty() {
        0
    } else {
        page.page_token
            .parse::<usize>()
            .map_err(|_| Status::invalid_argument("page_token 无效"))?
    };
    i64::try_from(offset).map_err(|_| Status::invalid_argument("page_token 超出结果范围"))?;
    Ok((limit, offset))
}

fn to_proto(record: &ConnectionDecisionRecord) -> Result<ConnectionDecisionSummary, Status> {
    let sequence =
        u64::try_from(record.sequence()).map_err(|_| Status::internal("决策序号无效"))?;
    Ok(ConnectionDecisionSummary {
        sequence,
        event_id: record.event_id().to_owned(),
        observed_at: Some(
            timestamp_from_unix_ms(record.occurred_at_unix_ms()).map_err(internal_status)?,
        ),
        app_platform: platform(record.app_platform()) as i32,
        app_stable_id: record.app_stable_id().to_owned(),
        app_display_name: record.application().to_owned(),
        destination: record.destination().to_owned(),
        destination_port: u32::from(record.destination_port()),
        transport: transport(record.transport()) as i32,
        action: action(record.action()) as i32,
        failure_mode: failure_mode(record.failure_mode()) as i32,
        matched_policy_id: record.matched_policy_id().unwrap_or_default().to_owned(),
        matched_rule_id: record.matched_rule_id().unwrap_or_default().to_owned(),
        reason_code: record.reason_code().to_owned(),
        evidence_level: evidence_level(record.evidence_level()) as i32,
        interface_name: record.interface_name().unwrap_or_default().to_owned(),
        outbound_id: record.outbound_id().unwrap_or_default().to_owned(),
        exit_probe_id: record.exit_probe_id().unwrap_or_default().to_owned(),
        decision_latency: record
            .decision_latency_micros()
            .map(duration_from_micros)
            .transpose()?,
        error_code: record.error_code().unwrap_or_default().to_owned(),
        snapshot_version: record.snapshot_version(),
        provider_id: record.provider_id().to_owned(),
        provider_generation: record.provider_generation(),
    })
}

const fn platform(value: Platform) -> common_proto::Platform {
    match value {
        Platform::MacOs => common_proto::Platform::Macos,
        Platform::Windows => common_proto::Platform::Windows,
    }
}

const fn transport(value: Transport) -> common_proto::TransportProtocol {
    match value {
        Transport::Tcp => common_proto::TransportProtocol::Tcp,
        Transport::Udp => common_proto::TransportProtocol::Udp,
    }
}

const fn action(value: RouteAction) -> common_proto::RouteAction {
    match value {
        RouteAction::Direct => common_proto::RouteAction::Direct,
        RouteAction::Proxy => common_proto::RouteAction::Proxy,
        RouteAction::Block => common_proto::RouteAction::Block,
    }
}

const fn failure_mode(value: FailureMode) -> common_proto::FailureMode {
    match value {
        FailureMode::Closed => common_proto::FailureMode::Closed,
        FailureMode::Open => common_proto::FailureMode::Open,
    }
}

const fn evidence_level(value: EvidenceLevel) -> common_proto::EvidenceLevel {
    match value {
        EvidenceLevel::Decision => common_proto::EvidenceLevel::Decision,
        EvidenceLevel::Path => common_proto::EvidenceLevel::Path,
        EvidenceLevel::Exit => common_proto::EvidenceLevel::Exit,
    }
}

fn duration_from_micros(value: u64) -> Result<prost_types::Duration, Status> {
    Ok(prost_types::Duration {
        seconds: i64::try_from(value / 1_000_000)
            .map_err(|_| Status::internal("决策耗时超出协议范围"))?,
        nanos: i32::try_from((value % 1_000_000) * 1_000)
            .map_err(|_| Status::internal("决策耗时超出协议范围"))?,
    })
}

#[cfg(test)]
mod tests;
