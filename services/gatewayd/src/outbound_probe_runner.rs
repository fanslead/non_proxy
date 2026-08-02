use std::{future::Future, sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_proto::events::v1::RuntimeState;
use nonproxy_storage::OutboundReference;

use crate::{
    Gateway,
    clock::unix_time_ms,
    credential_store::CredentialStore,
    flow_server::{FlowServiceError, outbound_factory::load_connector},
    outbound_probe_tls::authenticate_tls_path,
};

pub(crate) const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const PROBE_TARGET_HOST: &str = "example.com";
pub(crate) const PROBE_TARGET_PORT: u16 = 443;

#[derive(Debug)]
pub(crate) enum ProbeOutcome {
    Ready(Duration),
    Failed(FlowServiceError),
    TimedOut,
}

pub(crate) async fn run(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    outbound: OutboundReference,
    timeout: Duration,
) -> Result<ProbeOutcome, crate::GatewayError> {
    let probe_gateway = gateway.clone();
    observe_with(gateway, outbound, timeout, move |outbound_id, target| {
        let gateway = probe_gateway;
        let credentials = credential_store;
        async move {
            let connector = load_connector(&gateway, credentials, &outbound_id).await?;
            let requires_authenticated_path = connector.requires_authenticated_tls_probe();
            let stream = connector.connect_tcp(&target).await?;
            if requires_authenticated_path {
                authenticate_tls_path(stream, PROBE_TARGET_HOST).await?;
            }
            Ok(())
        }
    })
    .await
}

pub(crate) async fn observe_with<F, Fut>(
    gateway: &Gateway,
    outbound: OutboundReference,
    timeout: Duration,
    probe: F,
) -> Result<ProbeOutcome, crate::GatewayError>
where
    F: FnOnce(nonproxy_model::OutboundId, FlowEndpoint) -> Fut,
    Fut: Future<Output = Result<(), FlowServiceError>>,
{
    let target = FlowEndpoint::new(PROBE_TARGET_HOST, PROBE_TARGET_PORT)
        .map_err(|_| crate::GatewayError::InvalidContract("内置代理测试目标无效"))?;
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(timeout, probe(outbound.id().clone(), target)).await;
    let observed_at = unix_time_ms()?;

    match result {
        Ok(Ok(())) => {
            let elapsed = started.elapsed();
            gateway.report_outbound_health(
                outbound.id().clone(),
                outbound.revision(),
                RuntimeState::Ready,
                Some(duration_millis(elapsed)),
                observed_at,
            )?;
            Ok(ProbeOutcome::Ready(elapsed))
        }
        Ok(Err(error)) => {
            if records_outbound_failure(&error) {
                gateway.report_outbound_health(
                    outbound.id().clone(),
                    outbound.revision(),
                    RuntimeState::Failed,
                    None,
                    observed_at,
                )?;
            }
            Ok(ProbeOutcome::Failed(error))
        }
        Err(_) => {
            gateway.report_outbound_health(
                outbound.id().clone(),
                outbound.revision(),
                RuntimeState::Failed,
                None,
                observed_at,
            )?;
            Ok(ProbeOutcome::TimedOut)
        }
    }
}

fn records_outbound_failure(error: &FlowServiceError) -> bool {
    !matches!(
        error,
        FlowServiceError::SystemSnapshotPending
            | FlowServiceError::OutboundNotFound
            | FlowServiceError::OutboundDisabled
            | FlowServiceError::Gateway(_)
    )
}

fn duration_millis(value: Duration) -> u64 {
    let nanos = value.as_nanos();
    let rounded = nanos.saturating_add(999_999) / 1_000_000;
    u64::try_from(rounded).map_or(u64::MAX, |value| value)
}
