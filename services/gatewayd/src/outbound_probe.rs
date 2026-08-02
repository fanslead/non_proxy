use std::{sync::Arc, time::Duration};

#[cfg(test)]
use std::future::Future;

use crate::{
    Gateway, control_mapping,
    credential_store::CredentialStore,
    flow_server::FlowServiceError,
    outbound_probe_runner::{self, ProbeOutcome},
};
#[cfg(test)]
use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_model::OutboundId;
use nonproxy_proto::{
    common::v1::ErrorDetail,
    control::v1::{TestOutboundRequest, TestOutboundResponse},
};

const DEFAULT_PROBE_TIMEOUT: Duration = outbound_probe_runner::DEFAULT_PROBE_TIMEOUT;
const MINIMUM_PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const MAXIMUM_PROBE_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    request: TestOutboundRequest,
) -> TestOutboundResponse {
    let (timeout, outbound) = match request_parameters(gateway, request).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    response_from_outcome(
        outbound_probe_runner::run(gateway, credential_store, outbound, timeout).await,
    )
}

#[cfg(test)]
async fn run_with_probe<F, Fut>(
    gateway: &Gateway,
    request: TestOutboundRequest,
    probe: F,
) -> TestOutboundResponse
where
    F: FnOnce(OutboundId, FlowEndpoint) -> Fut,
    Fut: Future<Output = Result<(), FlowServiceError>>,
{
    let (timeout, outbound) = match request_parameters(gateway, request).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    response_from_outcome(
        outbound_probe_runner::observe_with(gateway, outbound, timeout, probe).await,
    )
}

async fn request_parameters(
    gateway: &Gateway,
    request: TestOutboundRequest,
) -> Result<(Duration, nonproxy_storage::OutboundReference), TestOutboundResponse> {
    let timeout = probe_timeout(request.timeout.as_ref()).map_err(response_error)?;
    let outbound_id = OutboundId::new(request.outbound_id)
        .map_err(|_| response_error(detail("NP_REQUEST_INVALID", "代理出口标识无效。", false)))?;
    let outbound = match gateway.outbound(outbound_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return Err(response_error(detail(
                "NP_FLOW_OUTBOUND_NOT_FOUND",
                "要测试的代理出口不存在，请刷新列表后重试。",
                false,
            )));
        }
        Err(error) => return Err(response_error(control_mapping::error_detail(&error))),
    };
    Ok((timeout, outbound))
}

fn response_from_outcome(
    outcome: Result<ProbeOutcome, crate::GatewayError>,
) -> TestOutboundResponse {
    match outcome {
        Ok(ProbeOutcome::Ready(elapsed)) => TestOutboundResponse {
            healthy: true,
            latency: Some(proto_duration(elapsed)),
            error: None,
        },
        Ok(ProbeOutcome::Failed(error)) => response_error(flow_detail(&error)),
        Ok(ProbeOutcome::TimedOut) => response_error(detail(
            "NP_OUTBOUND_TEST_TIMEOUT",
            "代理握手超时，请检查代理地址、端口和网络状态。",
            true,
        )),
        Err(error) => response_error(control_mapping::error_detail(&error)),
    }
}

fn probe_timeout(value: Option<&prost_types::Duration>) -> Result<Duration, ErrorDetail> {
    let value = match value {
        Some(value) => Duration::try_from(*value)
            .map_err(|_| detail("NP_REQUEST_INVALID", "代理测试超时时间格式无效。", false))?,
        None => DEFAULT_PROBE_TIMEOUT,
    };
    if !(MINIMUM_PROBE_TIMEOUT..=MAXIMUM_PROBE_TIMEOUT).contains(&value) {
        return Err(detail(
            "NP_REQUEST_INVALID",
            "代理测试超时时间必须在 1 到 30 秒之间。",
            false,
        ));
    }
    Ok(value)
}

fn flow_detail(error: &FlowServiceError) -> ErrorDetail {
    let message = match error {
        FlowServiceError::SystemSnapshotPending => {
            "防回环系统规则仍在激活中，请等待网络扩展加载后重试。"
        }
        FlowServiceError::OutboundNotFound => "要测试的代理出口不存在，请刷新列表后重试。",
        FlowServiceError::OutboundDisabled => "该代理出口已停用，请先启用后再测试。",
        FlowServiceError::OutboundUnsupported => "当前出口类型暂不支持内置握手测试。",
        FlowServiceError::OutboundInvalid => "代理出口配置不完整，请重新保存配置。",
        FlowServiceError::Credential(_) | FlowServiceError::CredentialTask => {
            "系统凭据库无法读取该代理的凭据或加密密钥。"
        }
        FlowServiceError::OutboundAuthentication => {
            "代理加密路径认证失败，请检查加密方法、密钥和服务端配置。"
        }
        FlowServiceError::Outbound(_) | FlowServiceError::Io(_) => {
            "代理握手失败，请检查地址、端口、认证信息和代理服务状态。"
        }
        FlowServiceError::Gateway(_) => "读取代理出口配置失败，请稍后重试。",
        _ => "代理握手未完成，请稍后重试。",
    };
    let retryable = matches!(
        error,
        FlowServiceError::SystemSnapshotPending
            | FlowServiceError::Io(_)
            | FlowServiceError::Outbound(_)
            | FlowServiceError::OutboundAuthentication
            | FlowServiceError::Gateway(_)
            | FlowServiceError::CredentialTask
    );
    detail(error.code(), message, retryable)
}

fn response_error(error: ErrorDetail) -> TestOutboundResponse {
    TestOutboundResponse {
        healthy: false,
        latency: None,
        error: Some(error),
    }
}

fn detail(code: &str, message: &str, retryable: bool) -> ErrorDetail {
    ErrorDetail {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable,
        metadata: Default::default(),
    }
}

fn proto_duration(value: Duration) -> prost_types::Duration {
    let seconds = i64::try_from(value.as_secs()).map_or(i64::MAX, |value| value);
    let nanos = i32::try_from(value.subsec_nanos()).map_or(i32::MAX, |value| value);
    prost_types::Duration { seconds, nanos }
}

#[cfg(test)]
mod tests {
    use std::{future, time::Duration};

    use nonproxy_model::OutboundId;
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_proto::{control::v1::TestOutboundRequest, events::v1::RuntimeState};
    use nonproxy_storage::{OutboundKind, OutboundReference, PolicyDatabase};

    use super::{flow_detail, probe_timeout, run_with_probe};
    use crate::{Gateway, flow_server::FlowServiceError};

    #[tokio::test]
    async fn successful_probe_reports_latency_and_updates_current_health() {
        let gateway = gateway_with_outbound().await;
        let response = run_with_probe(
            &gateway,
            test_request(2),
            |outbound_id, target| async move {
                assert_eq!(outbound_id.as_str(), "primary");
                assert_eq!(target.host(), "example.com");
                assert_eq!(target.port(), 443);
                Ok(())
            },
        )
        .await;

        assert!(response.healthy);
        assert!(response.error.is_none());
        assert!(response.latency.is_some());
        let outbounds = match gateway.list_outbounds().await {
            Ok(value) => value,
            Err(error) => panic!("读取测试出口失败: {error}"),
        };
        let health = gateway.outbound_health(&outbounds[0], current_time());
        assert!(matches!(
            health,
            Ok(Some(value))
                if value.state == RuntimeState::Ready && value.latency_ms.is_some()
        ));
    }

    #[tokio::test]
    async fn probe_rejects_out_of_range_timeout() {
        let gateway = gateway_with_outbound().await;
        let response = run_with_probe(&gateway, test_request(31), |_outbound_id, _target| async {
            Ok(())
        })
        .await;

        assert!(!response.healthy);
        assert!(matches!(
            response.error,
            Some(error) if error.code == "NP_REQUEST_INVALID"
                && error.message.contains("1 到 30 秒")
        ));
        let upper_bound = prost_types::Duration {
            seconds: 30,
            nanos: 0,
        };
        assert_eq!(
            probe_timeout(Some(&upper_bound)),
            Ok(Duration::from_secs(30))
        );
        assert_eq!(probe_timeout(None), Ok(Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn timed_out_probe_returns_retryable_error() {
        let gateway = gateway_with_outbound().await;
        let response = run_with_probe(&gateway, test_request(1), |_outbound_id, _target| {
            future::pending::<Result<(), FlowServiceError>>()
        })
        .await;

        assert!(!response.healthy);
        assert!(matches!(
            response.error,
            Some(error) if error.code == "NP_OUTBOUND_TEST_TIMEOUT" && error.retryable
        ));
        let outbounds = match gateway.list_outbounds().await {
            Ok(value) => value,
            Err(error) => panic!("读取超时测试出口失败: {error}"),
        };
        assert!(matches!(
            gateway.outbound_health(&outbounds[0], current_time()),
            Ok(Some(value)) if value.state == RuntimeState::Failed
                && value.latency_ms.is_none()
        ));
    }

    #[tokio::test]
    async fn failed_probe_returns_stable_error_and_failed_health() {
        let gateway = gateway_with_outbound().await;
        let response = run_with_probe(&gateway, test_request(2), |_outbound_id, _target| async {
            Err(FlowServiceError::OutboundInvalid)
        })
        .await;

        assert!(!response.healthy);
        assert!(response.latency.is_none());
        assert!(matches!(
            response.error,
            Some(error) if error.code == "NP_FLOW_OUTBOUND_INVALID"
                && !error.retryable
                && !error.message.contains("primary")
        ));
        let outbounds = match gateway.list_outbounds().await {
            Ok(value) => value,
            Err(error) => panic!("读取失败测试出口失败: {error}"),
        };
        assert!(matches!(
            gateway.outbound_health(&outbounds[0], current_time()),
            Ok(Some(value)) if value.state == RuntimeState::Failed
                && value.latency_ms.is_none()
        ));
    }

    #[tokio::test]
    async fn infrastructure_gate_does_not_mark_the_outbound_failed() {
        let gateway = gateway_with_outbound().await;
        let response = run_with_probe(&gateway, test_request(2), |_outbound_id, _target| async {
            Err(FlowServiceError::SystemSnapshotPending)
        })
        .await;

        assert!(!response.healthy);
        assert!(matches!(
            response.error,
            Some(error) if error.code == "NP_FLOW_SYSTEM_SNAPSHOT_PENDING" && error.retryable
        ));
        let outbounds = gateway
            .list_outbounds()
            .await
            .unwrap_or_else(|error| panic!("读取基础设施门测试出口失败: {error}"));
        assert!(matches!(
            gateway.outbound_health(&outbounds[0], current_time()),
            Ok(None)
        ));
    }

    #[test]
    fn authenticated_path_failure_has_a_stable_retryable_error_without_secrets() {
        let detail = flow_detail(&FlowServiceError::OutboundAuthentication);

        assert_eq!(detail.code, "NP_FLOW_OUTBOUND_AUTHENTICATION_FAILED");
        assert!(detail.retryable);
        assert!(detail.message.contains("加密路径认证失败"));
        assert!(!detail.message.contains("private"));
    }

    async fn gateway_with_outbound() -> Gateway {
        let database = match PolicyDatabase::open_in_memory(1) {
            Ok(value) => value,
            Err(error) => panic!("测试数据库打开失败: {error}"),
        };
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let id = match OutboundId::new("primary") {
            Ok(value) => value,
            Err(error) => panic!("测试出口 ID 创建失败: {error}"),
        };
        let outbound = match OutboundReference::new(
            id,
            OutboundKind::HttpConnect,
            Some("127.0.0.1"),
            Some(8_080),
            None,
            1,
        ) {
            Ok(value) => value,
            Err(error) => panic!("测试出口创建失败: {error}"),
        };
        if let Err(error) = gateway.save_outbounds(vec![(outbound, None)]).await {
            panic!("保存测试出口失败: {error}");
        }
        gateway
    }

    fn test_request(seconds: i64) -> TestOutboundRequest {
        TestOutboundRequest {
            context: None,
            outbound_id: "primary".to_owned(),
            timeout: Some(prost_types::Duration { seconds, nanos: 0 }),
        }
    }

    fn current_time() -> u64 {
        match crate::clock::unix_time_ms() {
            Ok(value) => value,
            Err(error) => panic!("读取测试时间失败: {error}"),
        }
    }
}
