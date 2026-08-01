use std::{sync::Arc, time::Duration};

use nonproxy_model::OutboundId;
use nonproxy_outbound::{
    ConnectorKind, OutboundConnector, ProxyCredentials, ProxyEndpoint, ShadowsocksCredentials,
    SystemTcpDialer, TcpDialer,
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
    let host = outbound
        .endpoint_host()
        .ok_or(FlowServiceError::OutboundInvalid)?;
    let port = outbound
        .endpoint_port()
        .ok_or(FlowServiceError::OutboundInvalid)?;
    let endpoint = ProxyEndpoint::new(host, port).map_err(|_| FlowServiceError::OutboundInvalid)?;
    match outbound.kind() {
        OutboundKind::Socks5 | OutboundKind::HttpConnect => {
            let kind = if outbound.kind() == OutboundKind::Socks5 {
                ConnectorKind::Socks5
            } else {
                ConnectorKind::HttpConnect
            };
            let credentials = match outbound.credential() {
                Some(reference) if reference.kind() == CredentialKind::Password => Some(
                    load_proxy_credentials(
                        Arc::clone(&credential_store),
                        reference.item_reference().to_owned(),
                    )
                    .await?,
                ),
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
        OutboundKind::Shadowsocks => {
            let reference = outbound
                .credential()
                .filter(|value| value.kind() == CredentialKind::Password)
                .ok_or(FlowServiceError::OutboundInvalid)?;
            let credentials = load_shadowsocks_credentials(
                credential_store,
                reference.item_reference().to_owned(),
            )
            .await?;
            Ok(OutboundConnector::shadowsocks_with_dialer(
                endpoint,
                credentials,
                CONNECT_TIMEOUT,
                dialer,
            ))
        }
        OutboundKind::Adapter => Err(FlowServiceError::OutboundUnsupported),
    }
}

async fn load_proxy_credentials(
    credential_store: Arc<dyn CredentialStore>,
    reference: String,
) -> Result<ProxyCredentials, FlowServiceError> {
    let encoded = load_credential(credential_store, reference).await?;
    ProxyCredentials::decode(encoded.as_slice()).map_err(FlowServiceError::from)
}

async fn load_shadowsocks_credentials(
    credential_store: Arc<dyn CredentialStore>,
    reference: String,
) -> Result<ShadowsocksCredentials, FlowServiceError> {
    let encoded = load_credential(credential_store, reference).await?;
    ShadowsocksCredentials::decode(encoded.as_slice()).map_err(FlowServiceError::from)
}

async fn load_credential(
    credential_store: Arc<dyn CredentialStore>,
    reference: String,
) -> Result<Zeroizing<Vec<u8>>, FlowServiceError> {
    let encoded = tokio::task::spawn_blocking(move || credential_store.get(&reference))
        .await
        .map_err(|_| FlowServiceError::CredentialTask)??;
    Ok(Zeroizing::new(encoded))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nonproxy_model::OutboundId;
    use nonproxy_outbound::ShadowsocksCredentials;
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_storage::{
        CredentialKind, CredentialReference, OutboundKind, OutboundReference, PolicyDatabase,
    };

    use super::load_connector;
    use crate::{
        Gateway,
        credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
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

    #[tokio::test]
    async fn loads_shadowsocks_from_the_versioned_password_reference() {
        let gateway = Gateway::new(
            PolicyDatabase::open_in_memory(1_000)
                .unwrap_or_else(|error| panic!("测试数据库打开失败: {error}")),
            CompileCapabilities::full(),
        );
        let id = OutboundId::new("modern-proxy")
            .unwrap_or_else(|error| panic!("Shadowsocks 出口标识创建失败: {error}"));
        let reference = "outbound:modern-proxy:v1:test";
        let credential =
            CredentialReference::new(reference, CredentialKind::Password, "Shadowsocks 密钥", 1)
                .unwrap_or_else(|error| panic!("Shadowsocks 凭据引用创建失败: {error}"));
        let outbound = OutboundReference::new(
            id.clone(),
            OutboundKind::Shadowsocks,
            Some("ss.example"),
            Some(8_388),
            Some(credential),
            1,
        )
        .unwrap_or_else(|error| panic!("Shadowsocks 出口创建失败: {error}"));
        gateway
            .save_outbounds(vec![(outbound, None)])
            .await
            .unwrap_or_else(|error| panic!("Shadowsocks 出口保存失败: {error}"));
        let credentials = ShadowsocksCredentials::new("aes-256-gcm", "private".to_owned())
            .unwrap_or_else(|error| panic!("Shadowsocks 测试密钥创建失败: {error}"));
        let store = Arc::new(MemoryCredentialStore::default());
        store
            .set(reference, credentials.encode().as_slice())
            .unwrap_or_else(|error| panic!("Shadowsocks 测试密钥保存失败: {error}"));

        let connector = load_connector(&gateway, store, &id)
            .await
            .unwrap_or_else(|error| panic!("Shadowsocks connector 加载失败: {error}"));

        assert!(connector.supports_udp());
        assert!(connector.requires_authenticated_tls_probe());
    }
}
