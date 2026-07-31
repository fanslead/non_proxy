use std::{sync::Arc, time::Duration};

use nonproxy_model::OutboundId;
use nonproxy_outbound::{
    ConnectorKind, OutboundConnector, ProxyCredentials, ProxyEndpoint, SystemTcpDialer, TcpDialer,
};
use nonproxy_storage::{CredentialKind, OutboundKind};
use zeroize::Zeroizing;

use crate::{Gateway, credential_store::CredentialStore};

use super::FlowServiceError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn load_connector(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    outbound_id: &OutboundId,
) -> Result<OutboundConnector, FlowServiceError> {
    load_connector_with_dialer(
        gateway,
        credential_store,
        outbound_id,
        Arc::new(SystemTcpDialer),
    )
    .await
}

pub async fn load_connector_with_dialer(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    outbound_id: &OutboundId,
    dialer: Arc<dyn TcpDialer>,
) -> Result<OutboundConnector, FlowServiceError> {
    if !gateway.system_snapshot_ready() {
        return Err(FlowServiceError::SystemSnapshotPending);
    }
    let outbound = gateway
        .outbound(outbound_id.clone())
        .await?
        .ok_or(FlowServiceError::OutboundNotFound)?;
    if !outbound.enabled() {
        return Err(FlowServiceError::OutboundDisabled);
    }
    let kind = match outbound.kind() {
        OutboundKind::Socks5 => ConnectorKind::Socks5,
        OutboundKind::HttpConnect => ConnectorKind::HttpConnect,
        OutboundKind::Adapter => return Err(FlowServiceError::OutboundUnsupported),
    };
    let host = outbound
        .endpoint_host()
        .ok_or(FlowServiceError::OutboundInvalid)?;
    let port = outbound
        .endpoint_port()
        .ok_or(FlowServiceError::OutboundInvalid)?;
    let endpoint = ProxyEndpoint::new(host, port).map_err(|_| FlowServiceError::OutboundInvalid)?;
    let credentials = match outbound.credential() {
        Some(reference) if reference.kind() == CredentialKind::Password => {
            Some(load_credentials(credential_store, reference.item_reference().to_owned()).await?)
        }
        Some(_) => return Err(FlowServiceError::OutboundInvalid),
        None => None,
    };
    Ok(OutboundConnector::with_dialer(
        kind,
        endpoint,
        credentials,
        CONNECT_TIMEOUT,
        dialer,
    ))
}

async fn load_credentials(
    credential_store: Arc<dyn CredentialStore>,
    reference: String,
) -> Result<ProxyCredentials, FlowServiceError> {
    let encoded = tokio::task::spawn_blocking(move || credential_store.get(&reference))
        .await
        .map_err(|_| FlowServiceError::CredentialTask)??;
    let encoded = Zeroizing::new(encoded);
    ProxyCredentials::decode(encoded.as_slice()).map_err(FlowServiceError::from)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nonproxy_model::OutboundId;
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_storage::PolicyDatabase;

    use super::load_connector;
    use crate::{
        Gateway, credential_store::tests_support::MemoryCredentialStore,
        flow_server::FlowServiceError,
    };

    #[tokio::test]
    async fn rejects_connector_loading_until_the_current_system_snapshot_is_active() {
        let gateway = Gateway::new(
            PolicyDatabase::open_in_memory(1_000)
                .unwrap_or_else(|error| panic!("测试数据库打开失败: {error}")),
            CompileCapabilities::full(),
        );
        gateway.set_system_snapshot_ready(false);
        let outbound_id = OutboundId::new("missing-proxy")
            .unwrap_or_else(|error| panic!("测试出口标识无效: {error}"));

        let result = load_connector(
            &gateway,
            Arc::new(MemoryCredentialStore::default()),
            &outbound_id,
        )
        .await;

        assert!(matches!(
            result,
            Err(FlowServiceError::SystemSnapshotPending)
        ));
    }
}
