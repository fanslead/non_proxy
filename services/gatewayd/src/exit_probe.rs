use std::{future::Future, net::IpAddr, sync::Arc, time::Duration};

use nonproxy_exit_probe::{ExitProbeClient, ExitProbeError, ProbeNonce, VerifiedExitProbe};
use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_model::OutboundId;
use nonproxy_outbound::BoxedProxyStream;
use nonproxy_proto::{
    common::v1::{ErrorDetail, IpFamily},
    control::v1::{ExitProbeRouteKind, VerifyExitRequest, VerifyExitResponse},
};

use crate::{
    Gateway,
    clock::{timestamp_from_unix_ms, unix_time_ms},
    credential_store::CredentialStore,
    exit_probe_direct::{DirectExitConnectError, connect as connect_direct},
    flow_server::{FlowServiceError, outbound_factory::load_connector},
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MINIMUM_TIMEOUT: Duration = Duration::from_secs(1);
const MAXIMUM_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
enum ProbeRoute {
    Direct,
    Proxy(OutboundId),
}

pub async fn run(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    client: Option<ExitProbeClient>,
    request: VerifyExitRequest,
) -> VerifyExitResponse {
    let client = match client {
        Some(value) => value,
        None => {
            return error_response(
                &request,
                detail(
                    "NP_EXIT_PROBE_NOT_CONFIGURED",
                    "当前安装尚未配置可信出口探针。",
                    false,
                ),
            );
        }
    };
    run_with_probe(request, move |route, timeout| {
        let gateway = gateway.clone();
        async move {
            let endpoint = FlowEndpoint::new(client.endpoint().host(), client.endpoint().port())
                .map_err(|_| RunError::Probe(ExitProbeError::Configuration))?;
            let nonce = ProbeNonce::generate().map_err(RunError::Probe)?;
            let stream: BoxedProxyStream = match route {
                ProbeRoute::Direct => Box::new(
                    connect_direct(&gateway, &endpoint)
                        .await
                        .map_err(RunError::Direct)?,
                ),
                ProbeRoute::Proxy(outbound_id) => {
                    let connector = load_connector(&gateway, credential_store, &outbound_id)
                        .await
                        .map_err(RunError::Flow)?;
                    connector
                        .connect_tcp(&endpoint)
                        .await
                        .map_err(|error| RunError::Flow(FlowServiceError::Outbound(error)))?
                }
            };
            let now = unix_time_ms().map_err(|_| RunError::Clock)?;
            client
                .probe(stream, nonce, now, timeout)
                .await
                .map_err(RunError::Probe)
        }
    })
    .await
}

async fn run_with_probe<F, Fut>(request: VerifyExitRequest, probe: F) -> VerifyExitResponse
where
    F: FnOnce(ProbeRoute, Duration) -> Fut,
    Fut: Future<Output = Result<VerifiedExitProbe, RunError>>,
{
    let timeout = match probe_timeout(request.timeout.as_ref()) {
        Ok(value) => value,
        Err(error) => return error_response(&request, error),
    };
    let route = match probe_route(request.route(), &request.outbound_id) {
        Ok(value) => value,
        Err(error) => return error_response(&request, error),
    };
    let result = tokio::time::timeout(timeout, probe(route.clone(), timeout)).await;
    match result {
        Ok(Ok(verified)) => match success_response(route, verified) {
            Ok(response) => response,
            Err(error) => error_response(&request, error),
        },
        Ok(Err(error)) => error_response(&request, run_error_detail(&error)),
        Err(_) => error_response(
            &request,
            detail(
                "NP_EXIT_PROBE_TIMEOUT",
                "出口验证超时，请检查网络后重试。",
                true,
            ),
        ),
    }
}

fn probe_timeout(value: Option<&prost_types::Duration>) -> Result<Duration, ErrorDetail> {
    let timeout = match value {
        Some(value) => Duration::try_from(*value)
            .map_err(|_| detail("NP_REQUEST_INVALID", "出口验证超时时间格式无效。", false))?,
        None => DEFAULT_TIMEOUT,
    };
    if !(MINIMUM_TIMEOUT..=MAXIMUM_TIMEOUT).contains(&timeout) {
        return Err(detail(
            "NP_REQUEST_INVALID",
            "出口验证超时时间必须在 1 到 30 秒之间。",
            false,
        ));
    }
    Ok(timeout)
}

fn probe_route(value: ExitProbeRouteKind, outbound_id: &str) -> Result<ProbeRoute, ErrorDetail> {
    match value {
        ExitProbeRouteKind::Direct if outbound_id.is_empty() => Ok(ProbeRoute::Direct),
        ExitProbeRouteKind::Proxy => OutboundId::new(outbound_id)
            .map(ProbeRoute::Proxy)
            .map_err(|_| detail("NP_REQUEST_INVALID", "代理出口标识无效。", false)),
        _ => Err(detail(
            "NP_REQUEST_INVALID",
            "出口验证路径与代理出口标识不一致。",
            false,
        )),
    }
}

fn success_response(
    route: ProbeRoute,
    verified: VerifiedExitProbe,
) -> Result<VerifyExitResponse, ErrorDetail> {
    let observed_ip = verified.observed_ip();
    let (route, outbound_id) = response_route(&route);
    let observed_at = timestamp_from_unix_ms(verified.observed_at_unix_ms()).map_err(|_| {
        detail(
            "NP_CLOCK_INVALID",
            "签名回执时间无法转换为本地时间。",
            false,
        )
    })?;
    Ok(VerifyExitResponse {
        verified: true,
        probe_id: verified.probe_id().to_owned(),
        observed_ip: observed_ip.to_string(),
        ip_family: match observed_ip {
            IpAddr::V4(_) => IpFamily::Ipv4 as i32,
            IpAddr::V6(_) => IpFamily::Ipv6 as i32,
        },
        observed_at: Some(observed_at),
        route: route as i32,
        outbound_id,
        error: None,
    })
}

fn error_response(request: &VerifyExitRequest, error: ErrorDetail) -> VerifyExitResponse {
    let route = request.route();
    VerifyExitResponse {
        verified: false,
        probe_id: String::new(),
        observed_ip: String::new(),
        ip_family: IpFamily::Unspecified as i32,
        observed_at: None,
        route: route as i32,
        outbound_id: request.outbound_id.clone(),
        error: Some(error),
    }
}

fn response_route(route: &ProbeRoute) -> (ExitProbeRouteKind, String) {
    match route {
        ProbeRoute::Direct => (ExitProbeRouteKind::Direct, String::new()),
        ProbeRoute::Proxy(outbound_id) => {
            (ExitProbeRouteKind::Proxy, outbound_id.as_str().to_owned())
        }
    }
}

#[derive(Debug)]
enum RunError {
    Clock,
    Direct(DirectExitConnectError),
    Flow(FlowServiceError),
    Probe(ExitProbeError),
}

fn run_error_detail(error: &RunError) -> ErrorDetail {
    match error {
        RunError::Clock => detail(
            "NP_CLOCK_INVALID",
            "系统时间无效，无法验证签名回执。",
            false,
        ),
        RunError::Direct(DirectExitConnectError::SystemSnapshotPending)
        | RunError::Flow(FlowServiceError::SystemSnapshotPending) => detail(
            "NP_EXIT_PROBE_SYSTEM_SNAPSHOT_PENDING",
            "防回环系统规则仍在激活中，请稍后重试。",
            true,
        ),
        #[cfg(windows)]
        RunError::Direct(DirectExitConnectError::PhysicalInterfaceUnavailable) => detail(
            "NP_EXIT_PROBE_PHYSICAL_INTERFACE_UNAVAILABLE",
            "没有可用的物理网络接口，无法验证直连出口。",
            true,
        ),
        RunError::Direct(DirectExitConnectError::Connect) => detail(
            "NP_EXIT_PROBE_DIRECT_CONNECT_FAILED",
            "直连出口无法连接可信探针。",
            true,
        ),
        RunError::Flow(error) => flow_error_detail(error),
        RunError::Probe(error) => probe_error_detail(error),
    }
}

fn flow_error_detail(error: &FlowServiceError) -> ErrorDetail {
    let (message, retryable) = match error {
        FlowServiceError::OutboundNotFound => ("要验证的代理出口不存在。", false),
        FlowServiceError::OutboundDisabled => ("要验证的代理出口已停用。", false),
        FlowServiceError::OutboundUnsupported => ("当前代理出口类型不支持出口验证。", false),
        FlowServiceError::OutboundInvalid => ("代理出口配置不完整。", false),
        FlowServiceError::Credential(_) => ("代理出口凭据不可用。", false),
        FlowServiceError::CredentialTask => ("系统凭据库暂时不可用。", true),
        FlowServiceError::Outbound(_) | FlowServiceError::Io(_) => {
            ("代理出口无法连接可信探针，请检查代理网络。", true)
        }
        FlowServiceError::Gateway(_) => ("读取代理出口配置失败，请稍后重试。", true),
        _ => ("代理出口验证未完成，请稍后重试。", true),
    };
    detail(error.code(), message, retryable)
}

fn probe_error_detail(error: &ExitProbeError) -> ErrorDetail {
    let (code, message, retryable) = match error {
        ExitProbeError::Timeout => (
            "NP_EXIT_PROBE_TIMEOUT",
            "出口验证超时，请检查网络后重试。",
            true,
        ),
        ExitProbeError::Connect | ExitProbeError::Http | ExitProbeError::HttpStatus => (
            "NP_EXIT_PROBE_REMOTE_UNAVAILABLE",
            "可信出口探针暂时不可用。",
            true,
        ),
        ExitProbeError::Tls => (
            "NP_EXIT_PROBE_TLS_INVALID",
            "出口探针 TLS 身份验证失败。",
            false,
        ),
        ExitProbeError::SignatureInvalid
        | ExitProbeError::NonceMismatch
        | ExitProbeError::KeyInvalid => (
            "NP_EXIT_PROBE_SIGNATURE_INVALID",
            "出口探针签名回执验证失败。",
            false,
        ),
        ExitProbeError::TimestampInvalid => (
            "NP_EXIT_PROBE_RECEIPT_EXPIRED",
            "出口探针签名回执时间无效。",
            false,
        ),
        ExitProbeError::AddressInvalid => (
            "NP_EXIT_PROBE_ADDRESS_INVALID",
            "出口探针未返回公网地址。",
            false,
        ),
        _ => (
            "NP_EXIT_PROBE_RESPONSE_INVALID",
            "出口探针返回了无效回执。",
            false,
        ),
    };
    detail(code, message, retryable)
}

fn detail(code: &str, message: &str, retryable: bool) -> ErrorDetail {
    ErrorDetail {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable,
        metadata: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::{net::IpAddr, str::FromStr, time::Duration};

    use nonproxy_exit_probe::{ExitProbeSigner, ExitProbeVerifier, ProbeNonce};
    use nonproxy_proto::control::v1::{ExitProbeRouteKind, VerifyExitRequest};

    use super::{ProbeRoute, RunError, run_with_probe};

    #[tokio::test]
    async fn returns_verified_signed_exit_for_selected_proxy() {
        let response = run_with_probe(
            request(ExitProbeRouteKind::Proxy, "primary"),
            |route, _| async move {
                assert!(matches!(
                    route,
                    ProbeRoute::Proxy(ref id) if id.as_str() == "primary"
                ));
                Ok(verified_fixture())
            },
        )
        .await;

        assert!(response.verified);
        assert_eq!(response.observed_ip, "8.8.8.8");
        assert_eq!(response.route(), ExitProbeRouteKind::Proxy);
        assert_eq!(response.outbound_id, "primary");
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn rejects_inconsistent_route_without_running_probe() {
        let response = run_with_probe(
            request(ExitProbeRouteKind::Direct, "must-be-empty"),
            |_route, _| async { Err(RunError::Clock) },
        )
        .await;

        assert!(!response.verified);
        assert!(matches!(
            response.error,
            Some(error) if error.code == "NP_REQUEST_INVALID"
        ));
    }

    fn request(route: ExitProbeRouteKind, outbound_id: &str) -> VerifyExitRequest {
        VerifyExitRequest {
            context: None,
            route: route as i32,
            outbound_id: outbound_id.to_owned(),
            timeout: Some(
                prost_types::Duration::try_from(Duration::from_secs(2))
                    .unwrap_or_else(|error| panic!("测试超时转换失败: {error}")),
            ),
        }
    }

    fn verified_fixture() -> nonproxy_exit_probe::VerifiedExitProbe {
        let secret = [7_u8; 32];
        let signer = ExitProbeSigner::from_secret_bytes(&secret)
            .unwrap_or_else(|error| panic!("测试签名器创建失败: {error}"));
        let verifier = ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64())
            .unwrap_or_else(|error| panic!("测试验证器创建失败: {error}"));
        let nonce = ProbeNonce::from_base64("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            .unwrap_or_else(|error| panic!("测试 nonce 无效: {error}"));
        let observed_at = 10_000;
        let ip =
            IpAddr::from_str("8.8.8.8").unwrap_or_else(|error| panic!("测试地址无效: {error}"));
        let receipt = signer
            .sign(nonce, ip, observed_at)
            .unwrap_or_else(|error| panic!("测试回执签名失败: {error}"));
        verifier
            .verify(nonce, receipt, observed_at)
            .unwrap_or_else(|error| panic!("测试回执验证失败: {error}"))
    }
}
