use std::{collections::HashSet, net::SocketAddr, sync::Arc, time::Duration};

use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use http_body_util::{BodyExt as _, Empty};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_rustls::TlsConnector;
use zeroize::Zeroizing;

use crate::{SubscriptionEndpoint, SubscriptionFetchError, is_public_destination};

pub const MAXIMUM_SUBSCRIPTION_BYTES: usize = 256 * 1024;
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct SubscriptionClient {
    tls: Arc<ClientConfig>,
}

impl SubscriptionClient {
    #[must_use]
    pub fn new() -> Self {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Self { tls: Arc::new(tls) }
    }

    pub async fn fetch(
        &self,
        endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError> {
        tokio::time::timeout(FETCH_TIMEOUT, self.fetch_inner(endpoint))
            .await
            .map_err(|_| SubscriptionFetchError::Timeout)?
    }

    async fn fetch_inner(
        &self,
        endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError> {
        let addresses = resolve_public_addresses(endpoint).await?;
        let mut last_error = SubscriptionFetchError::Connect;
        for address in addresses {
            let stream = match TcpStream::connect(address).await {
                Ok(value) => value,
                Err(_) => continue,
            };
            let _result = stream.set_nodelay(true);
            match self.fetch_from_stream(endpoint, stream).await {
                Ok(value) => return Ok(value),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    async fn fetch_from_stream<S>(
        &self,
        endpoint: &SubscriptionEndpoint,
        stream: S,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
    {
        let server_name = ServerName::try_from(endpoint.host().to_owned())
            .map_err(|_| SubscriptionFetchError::EndpointInvalid)?;
        let tls = TlsConnector::from(Arc::clone(&self.tls))
            .connect(server_name, stream)
            .await
            .map_err(|_| SubscriptionFetchError::Tls)?;
        let (mut sender, connection) = http1::handshake(TokioIo::new(tls))
            .await
            .map_err(|_| SubscriptionFetchError::Http)?;
        let _connection_task = AbortTaskOnDrop(tokio::spawn(async move {
            let _result = connection.await;
        }));
        let request = Request::builder()
            .method(Method::GET)
            .uri(endpoint.path_and_query())
            .header(header::HOST, endpoint.host_header())
            .header(header::ACCEPT, "text/plain, application/octet-stream")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::USER_AGENT, "NonProxy-Subscription/1")
            .body(Empty::<Bytes>::new())
            .map_err(|_| SubscriptionFetchError::Http)?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| SubscriptionFetchError::Http)?;
        if response.status() != StatusCode::OK {
            return Err(SubscriptionFetchError::HttpStatus);
        }
        if response
            .headers()
            .get_all(header::CONTENT_ENCODING)
            .iter()
            .any(|value| value.as_bytes() != b"identity")
        {
            return Err(SubscriptionFetchError::ContentEncoding);
        }
        if response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|value| value > MAXIMUM_SUBSCRIPTION_BYTES)
        {
            return Err(SubscriptionFetchError::ResponseTooLarge);
        }

        let mut incoming = response.into_body();
        let mut body = Zeroizing::new(Vec::new());
        while let Some(frame) = incoming.frame().await {
            let frame = frame.map_err(|_| SubscriptionFetchError::Http)?;
            if let Ok(data) = frame.into_data() {
                let next_length = body
                    .len()
                    .checked_add(data.len())
                    .ok_or(SubscriptionFetchError::ResponseTooLarge)?;
                if next_length > MAXIMUM_SUBSCRIPTION_BYTES {
                    return Err(SubscriptionFetchError::ResponseTooLarge);
                }
                body.extend_from_slice(&data);
            }
        }
        if body.is_empty() {
            return Err(SubscriptionFetchError::Http);
        }
        Ok(body)
    }
}

impl Default for SubscriptionClient {
    fn default() -> Self {
        Self::new()
    }
}

async fn resolve_public_addresses(
    endpoint: &SubscriptionEndpoint,
) -> Result<Vec<SocketAddr>, SubscriptionFetchError> {
    let resolved = tokio::net::lookup_host((endpoint.host(), endpoint.port()))
        .await
        .map_err(|_| SubscriptionFetchError::Resolve)?;
    validate_resolved_addresses(resolved)
}

fn validate_resolved_addresses<I>(resolved: I) -> Result<Vec<SocketAddr>, SubscriptionFetchError>
where
    I: IntoIterator<Item = SocketAddr>,
{
    let mut unique = HashSet::new();
    for address in resolved {
        if !is_public_destination(address.ip()) {
            return Err(SubscriptionFetchError::AddressNotPublic);
        }
        unique.insert(address);
    }
    if unique.is_empty() {
        return Err(SubscriptionFetchError::AddressNotPublic);
    }
    let mut addresses = unique.into_iter().collect::<Vec<_>>();
    addresses.sort_unstable();
    Ok(addresses)
}

struct AbortTaskOnDrop(JoinHandle<()>);

impl Drop for AbortTaskOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use bytes::Bytes;
    use http::{Request, Response, StatusCode, header};
    use http_body_util::Full;
    use hyper::{body::Incoming, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use rcgen::{CertifiedKey, generate_simple_self_signed};
    use rustls::{
        ClientConfig, RootCertStore, ServerConfig,
        pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
    };
    use tokio::io::duplex;
    use tokio_rustls::TlsAcceptor;

    use super::{MAXIMUM_SUBSCRIPTION_BYTES, SubscriptionClient, validate_resolved_addresses};
    use crate::{SubscriptionEndpoint, SubscriptionFetchError};

    #[tokio::test]
    async fn tls_fetch_returns_bounded_payload_without_sending_the_token_as_metadata() {
        let (client, server) = client_and_server();
        let endpoint = SubscriptionEndpoint::parse("https://localhost/nodes?token=private")
            .unwrap_or_else(|error| panic!("测试订阅地址解析失败: {error}"));
        let (client_stream, server_stream) = duplex(16 * 1024);
        let server_task = tokio::spawn(serve_once(server, server_stream, |request| {
            assert_eq!(
                request.uri().path_and_query().map(|value| value.as_str()),
                Some("/nodes?token=private")
            );
            assert_eq!(
                request
                    .headers()
                    .get(header::CACHE_CONTROL)
                    .and_then(|value| value.to_str().ok()),
                Some("no-store")
            );
            Response::new(Full::new(Bytes::from_static(b"subscription-payload")))
        }));

        let payload = client
            .fetch_from_stream(&endpoint, client_stream)
            .await
            .unwrap_or_else(|error| panic!("测试订阅获取失败: {error}"));

        assert_eq!(payload.as_slice(), b"subscription-payload");
        server_task
            .await
            .unwrap_or_else(|error| panic!("测试订阅服务任务失败: {error}"));
    }

    #[tokio::test]
    async fn rejects_redirect_compression_and_oversized_streaming_bodies() {
        let redirect = Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, "https://other.example/private")
            .body(Full::new(Bytes::new()))
            .unwrap_or_else(|error| panic!("测试重定向响应构造失败: {error}"));
        assert!(matches!(
            fetch_response(redirect).await,
            SubscriptionFetchError::HttpStatus
        ));

        let compressed = Response::builder()
            .header(header::CONTENT_ENCODING, "gzip")
            .body(Full::new(Bytes::from_static(b"compressed")))
            .unwrap_or_else(|error| panic!("测试压缩响应构造失败: {error}"));
        assert!(matches!(
            fetch_response(compressed).await,
            SubscriptionFetchError::ContentEncoding
        ));

        let oversized = Response::builder()
            .body(Full::new(Bytes::from(vec![
                b'x';
                MAXIMUM_SUBSCRIPTION_BYTES + 1
            ])))
            .unwrap_or_else(|error| panic!("测试超限响应构造失败: {error}"));
        assert!(matches!(
            fetch_response(oversized).await,
            SubscriptionFetchError::ResponseTooLarge
        ));
    }

    #[tokio::test]
    async fn private_literal_is_rejected_before_connecting() {
        let endpoint = SubscriptionEndpoint::parse("https://127.0.0.1/nodes")
            .unwrap_or_else(|error| panic!("测试订阅地址解析失败: {error}"));
        let error = SubscriptionClient::new()
            .fetch(&endpoint)
            .await
            .err()
            .unwrap_or(SubscriptionFetchError::Http);

        assert!(matches!(error, SubscriptionFetchError::AddressNotPublic));
    }

    #[test]
    fn mixed_public_and_private_dns_answers_fail_closed() {
        let public = "93.184.216.34:443"
            .parse()
            .unwrap_or_else(|error| panic!("测试公网地址解析失败: {error}"));
        let private = "127.0.0.1:443"
            .parse()
            .unwrap_or_else(|error| panic!("测试私网地址解析失败: {error}"));

        assert!(matches!(
            validate_resolved_addresses([public, private]),
            Err(SubscriptionFetchError::AddressNotPublic)
        ));
    }

    async fn fetch_response(response: Response<Full<Bytes>>) -> SubscriptionFetchError {
        let (client, server) = client_and_server();
        let endpoint = SubscriptionEndpoint::parse("https://localhost/nodes")
            .unwrap_or_else(|error| panic!("测试订阅地址解析失败: {error}"));
        let (client_stream, server_stream) = duplex(MAXIMUM_SUBSCRIPTION_BYTES + 4 * 1024);
        let server_task = tokio::spawn(serve_once(server, server_stream, move |_| response));
        let result = client.fetch_from_stream(&endpoint, client_stream).await;
        let _server_result = server_task.await;
        result.err().unwrap_or(SubscriptionFetchError::Http)
    }

    fn client_and_server() -> (SubscriptionClient, Arc<ServerConfig>) {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["localhost".to_owned()])
                .unwrap_or_else(|error| panic!("测试 TLS 证书生成失败: {error}"));
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
        let server = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], key)
            .unwrap_or_else(|error| panic!("测试 TLS 服务配置失败: {error}"));
        let mut roots = RootCertStore::empty();
        roots
            .add(cert.der().clone())
            .unwrap_or_else(|error| panic!("测试 TLS 根证书添加失败: {error}"));
        let tls = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        (SubscriptionClient { tls: Arc::new(tls) }, Arc::new(server))
    }

    async fn serve_once<F>(server: Arc<ServerConfig>, stream: tokio::io::DuplexStream, handler: F)
    where
        F: FnOnce(Request<Incoming>) -> Response<Full<Bytes>> + Send + 'static,
    {
        let tls = TlsAcceptor::from(server)
            .accept(stream)
            .await
            .unwrap_or_else(|error| panic!("测试 TLS 服务握手失败: {error}"));
        let handler = Arc::new(std::sync::Mutex::new(Some(handler)));
        let service = service_fn(move |request| {
            let handler = Arc::clone(&handler);
            async move {
                let response = handler
                    .lock()
                    .ok()
                    .and_then(|mut value| value.take())
                    .map_or_else(
                        || Response::new(Full::new(Bytes::new())),
                        |value| value(request),
                    );
                Ok::<_, Infallible>(response)
            }
        });
        let _result = http1::Builder::new()
            .serve_connection(TokioIo::new(tls), service)
            .await;
    }
}
