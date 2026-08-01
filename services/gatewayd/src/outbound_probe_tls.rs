use std::sync::Arc;

use nonproxy_outbound::BoxedProxyStream;
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio_rustls::TlsConnector;

use crate::flow_server::FlowServiceError;

pub(crate) async fn authenticate_tls_path(
    stream: BoxedProxyStream,
    target_host: &str,
) -> Result<(), FlowServiceError> {
    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(target_host.to_owned())
        .map_err(|_| FlowServiceError::OutboundAuthentication)?;
    TlsConnector::from(Arc::new(config))
        .connect(server_name, stream)
        .await
        .map_err(|_| FlowServiceError::OutboundAuthentication)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use nonproxy_outbound::BoxedProxyStream;

    use super::authenticate_tls_path;
    use crate::flow_server::FlowServiceError;

    #[tokio::test]
    async fn invalid_tls_peer_maps_to_the_stable_authentication_error() {
        let (client, server) = tokio::io::duplex(4_096);
        drop(server);
        let stream: BoxedProxyStream = Box::new(client);

        let result = authenticate_tls_path(stream, "example.com").await;

        assert!(matches!(
            result,
            Err(FlowServiceError::OutboundAuthentication)
        ));
    }
}
