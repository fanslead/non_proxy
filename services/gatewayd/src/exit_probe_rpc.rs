use std::net::IpAddr;

use nonproxy_proto::{
    common::v1::{IpFamily, PageRequest, PageResponse},
    control::v1::{
        ExitProbeRouteKind, ExitProbeSummary, ListExitProbesRequest, ListExitProbesResponse,
    },
};
use nonproxy_storage::{ExitProbeRecord, ExitProbeRoute};
use tonic::Status;

use crate::{Gateway, clock::timestamp_from_unix_ms, control_rpc_helpers::internal_status};

const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE_SIZE: usize = 200;

pub async fn list(
    gateway: &Gateway,
    request: ListExitProbesRequest,
    verification_available: bool,
) -> Result<ListExitProbesResponse, Status> {
    let (limit, offset) = parse_page(request.page)?;
    let (records, total) = gateway
        .list_exit_probes(limit, offset)
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
    Ok(ListExitProbesResponse {
        probes: records
            .iter()
            .map(to_proto)
            .collect::<Result<Vec<_>, _>>()?,
        page: Some(PageResponse { next_page_token }),
        total_count: total,
        verification_available,
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

fn to_proto(record: &ExitProbeRecord) -> Result<ExitProbeSummary, Status> {
    let sequence =
        u64::try_from(record.sequence()).map_err(|_| Status::internal("出口回执序号无效"))?;
    let (route, outbound_id) = route(record.route());
    let observed_ip = record.observed_ip();
    Ok(ExitProbeSummary {
        sequence,
        probe_id: record.probe_id().to_owned(),
        route: route as i32,
        outbound_id,
        observed_ip: observed_ip.to_string(),
        ip_family: ip_family(observed_ip) as i32,
        observed_at: Some(
            timestamp_from_unix_ms(record.observed_at_unix_ms()).map_err(internal_status)?,
        ),
        key_id: record.key_id().to_owned(),
        verified_at: Some(
            timestamp_from_unix_ms(record.verified_at_unix_ms()).map_err(internal_status)?,
        ),
    })
}

fn route(value: &ExitProbeRoute) -> (ExitProbeRouteKind, String) {
    match value {
        ExitProbeRoute::Direct => (ExitProbeRouteKind::Direct, String::new()),
        ExitProbeRoute::Proxy(outbound_id) => {
            (ExitProbeRouteKind::Proxy, outbound_id.as_str().to_owned())
        }
    }
}

const fn ip_family(value: IpAddr) -> IpFamily {
    match value {
        IpAddr::V4(_) => IpFamily::Ipv4,
        IpAddr::V6(_) => IpFamily::Ipv6,
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::common::v1::PageRequest;

    use super::parse_page;

    #[test]
    fn page_contract_is_bounded_and_rejects_non_numeric_tokens() {
        assert!(matches!(parse_page(None), Ok((100, 0))));
        assert!(
            parse_page(Some(PageRequest {
                page_size: 201,
                page_token: String::new(),
            }))
            .is_err()
        );
        assert!(
            parse_page(Some(PageRequest {
                page_size: 10,
                page_token: "not-a-number".to_owned(),
            }))
            .is_err()
        );
    }
}
