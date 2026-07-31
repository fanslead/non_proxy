use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use http_body_util::{BodyExt as _, Empty, Limited};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;
use url::Url;

use crate::{
    ExitProbeError, ExitProbeReceipt, ExitProbeVerifierSet, ProbeNonce, VerifiedExitProbe,
};

const MAXIMUM_RESPONSE_BYTES: usize = 4 * 1024;
const DEFAULT_PORT: u16 = 443;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitProbeEndpoint {
    host: String,
    port: u16,
    path: String,
}

impl ExitProbeEndpoint {
    pub fn parse(value: &str) -> Result<Self, ExitProbeError> {
        let url = Url::parse(value).map_err(|_| ExitProbeError::Configuration)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ExitProbeError::Configuration);
        }
        let host = url
            .host_str()
            .filter(|value| !value.is_empty())
            .ok_or(ExitProbeError::Configuration)?
            .to_owned();
        let port = url.port_or_known_default().unwrap_or(DEFAULT_PORT);
        let path = match url.path() {
            "" | "/" => "/v1/exit".to_owned(),
            value => value.to_owned(),
        };
        Ok(Self { host, port, path })
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

#[derive(Clone)]
pub struct ExitProbeClient {
    endpoint: ExitProbeEndpoint,
    verifiers: ExitProbeVerifierSet,
    tls: Arc<ClientConfig>,
}

impl ExitProbeClient {
    pub fn new<V>(endpoint: ExitProbeEndpoint, verifiers: V) -> Result<Self, ExitProbeError>
    where
        V: Into<ExitProbeVerifierSet>,
    {
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Self {
            endpoint,
            verifiers: verifiers.into(),
            tls: Arc::new(tls),
        })
    }

    #[must_use]
    pub const fn endpoint(&self) -> &ExitProbeEndpoint {
        &self.endpoint
    }

    pub async fn probe<S>(
        &self,
        stream: S,
        nonce: ProbeNonce,
        now_unix_ms: u64,
        timeout: Duration,
    ) -> Result<VerifiedExitProbe, ExitProbeError>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        tokio::time::timeout(timeout, self.probe_inner(stream, nonce, now_unix_ms))
            .await
            .map_err(|_| ExitProbeError::Timeout)?
    }

    async fn probe_inner<S>(
        &self,
        stream: S,
        nonce: ProbeNonce,
        now_unix_ms: u64,
    ) -> Result<VerifiedExitProbe, ExitProbeError>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let server_name = ServerName::try_from(self.endpoint.host.clone())
            .map_err(|_| ExitProbeError::Configuration)?;
        let tls = TlsConnector::from(Arc::clone(&self.tls))
            .connect(server_name, stream)
            .await
            .map_err(|_| ExitProbeError::Tls)?;
        let (mut sender, connection) = http1::handshake(TokioIo::new(tls))
            .await
            .map_err(|_| ExitProbeError::Http)?;
        let _connection_task = AbortTaskOnDrop(tokio::spawn(async move {
            let _result = connection.await;
        }));
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "{}?nonce={}",
                self.endpoint.path,
                nonce.to_base64()
            ))
            .header(header::HOST, host_header(&self.endpoint))
            .header(header::ACCEPT, "application/json")
            .header(header::USER_AGENT, "NonProxy-Exit-Probe/1")
            .body(Empty::<Bytes>::new())
            .map_err(|_| ExitProbeError::Http)?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|_| ExitProbeError::Http)?;
        if response.status() != StatusCode::OK {
            return Err(ExitProbeError::HttpStatus);
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        if content_type != Some("application/json") {
            return Err(ExitProbeError::ResponseInvalid);
        }
        let body = Limited::new(response.into_body(), MAXIMUM_RESPONSE_BYTES)
            .collect()
            .await
            .map_err(|error| {
                if error.is::<http_body_util::LengthLimitError>() {
                    ExitProbeError::ResponseTooLarge
                } else {
                    ExitProbeError::Http
                }
            })?
            .to_bytes();
        let receipt = serde_json::from_slice::<ExitProbeReceipt>(&body)
            .map_err(|_| ExitProbeError::ResponseInvalid)?;
        self.verifiers.verify(nonce, receipt, now_unix_ms)
    }
}

struct AbortTaskOnDrop(JoinHandle<()>);

impl Drop for AbortTaskOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn host_header(endpoint: &ExitProbeEndpoint) -> String {
    if endpoint.port == DEFAULT_PORT {
        endpoint.host.clone()
    } else if endpoint.host.contains(':') {
        format!("[{}]:{}", endpoint.host, endpoint.port)
    } else {
        format!("{}:{}", endpoint.host, endpoint.port)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        future::pending,
        net::Ipv4Addr,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    use bytes::Bytes;
    use http::{Request, Response};
    use http_body_util::Full;
    use hyper::{body::Incoming, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use rcgen::{CertifiedKey, generate_simple_self_signed};
    use rustls::{
        ClientConfig, RootCertStore, ServerConfig,
        pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
    };
    use tokio::{io::duplex, sync::Notify};
    use tokio_rustls::TlsAcceptor;

    use super::{AbortTaskOnDrop, ExitProbeClient, ExitProbeEndpoint};
    use crate::{ExitProbeSigner, ExitProbeVerifier, ProbeNonce};

    #[test]
    fn endpoint_requires_https_without_credentials_or_query() {
        let endpoint = ExitProbeEndpoint::parse("https://probe.example/v1/exit");
        let Ok(endpoint) = endpoint else {
            panic!("合法探针 endpoint 解析失败: {endpoint:?}");
        };

        assert_eq!(endpoint.host(), "probe.example");
        assert_eq!(endpoint.port(), 443);
        assert!(ExitProbeEndpoint::parse("http://probe.example").is_err());
        assert!(ExitProbeEndpoint::parse("https://user@probe.example").is_err());
        assert!(ExitProbeEndpoint::parse("https://probe.example?target=secret").is_err());
    }

    #[tokio::test]
    async fn tls_http_round_trip_returns_a_verified_signed_receipt() {
        let CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(vec!["localhost".to_owned()])
                .unwrap_or_else(|error| panic!("测试 TLS 证书生成失败: {error}"));
        let server_key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
        let server_tls = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], server_key)
            .unwrap_or_else(|error| panic!("测试 TLS 服务配置失败: {error}"));
        let mut roots = RootCertStore::empty();
        roots
            .add(cert.der().clone())
            .unwrap_or_else(|error| panic!("测试 TLS 根证书添加失败: {error}"));
        let client_tls = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let receipt_signer = Arc::new(
            ExitProbeSigner::from_secret_bytes(&[7; 32])
                .unwrap_or_else(|error| panic!("测试回执签名器创建失败: {error}")),
        );
        let verifier =
            ExitProbeVerifier::from_public_key_base64(&receipt_signer.public_key_base64())
                .unwrap_or_else(|error| panic!("测试回执验证器创建失败: {error}"));
        let endpoint = ExitProbeEndpoint::parse("https://localhost/v1/exit")
            .unwrap_or_else(|error| panic!("测试 endpoint 解析失败: {error}"));
        let client = ExitProbeClient {
            endpoint,
            verifiers: verifier.into(),
            tls: Arc::new(client_tls),
        };
        let nonce = ProbeNonce::from_base64("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            .unwrap_or_else(|error| panic!("测试 nonce 创建失败: {error}"));
        let observed_at = 10_000;
        let (client_stream, server_stream) = duplex(16 * 1024);
        let server = tokio::spawn(async move {
            let acceptor = TlsAcceptor::from(Arc::new(server_tls));
            let tls = acceptor
                .accept(server_stream)
                .await
                .unwrap_or_else(|error| panic!("测试 TLS 握手失败: {error}"));
            let service = service_fn(move |request: Request<Incoming>| {
                let signer = Arc::clone(&receipt_signer);
                async move {
                    let nonce_text = request
                        .uri()
                        .query()
                        .and_then(|value| value.strip_prefix("nonce="))
                        .unwrap_or_default();
                    let nonce = ProbeNonce::from_base64(nonce_text)
                        .unwrap_or_else(|error| panic!("测试请求 nonce 无效: {error}"));
                    let receipt = signer
                        .sign(nonce, Ipv4Addr::new(8, 8, 8, 8).into(), observed_at)
                        .unwrap_or_else(|error| panic!("测试回执签名失败: {error}"));
                    let body = serde_json::to_vec(&receipt)
                        .unwrap_or_else(|error| panic!("测试回执编码失败: {error}"));
                    let mut response = Response::new(Full::new(Bytes::from(body)));
                    response.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );
                    Ok::<_, Infallible>(response)
                }
            });
            let _result = http1::Builder::new()
                .serve_connection(TokioIo::new(tls), service)
                .await;
        });

        let verified = client
            .probe(client_stream, nonce, observed_at, Duration::from_secs(2))
            .await
            .unwrap_or_else(|error| panic!("TLS 出口探针回执验证失败: {error}"));
        server
            .await
            .unwrap_or_else(|error| panic!("测试 TLS 服务任务失败: {error}"));

        assert_eq!(verified.observed_ip(), Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(verified.observed_at_unix_ms(), observed_at);
        assert_eq!(verified.probe_id().len(), 43);
    }

    #[tokio::test]
    async fn aborts_the_connection_driver_when_a_request_future_is_dropped() {
        struct DropMarker(Arc<AtomicBool>);

        impl Drop for DropMarker {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let started = Arc::new(Notify::new());
        let task_dropped = Arc::clone(&dropped);
        let task_started = Arc::clone(&started);
        let task = AbortTaskOnDrop(tokio::spawn(async move {
            let _marker = DropMarker(task_dropped);
            task_started.notify_one();
            pending::<()>().await;
        }));
        started.notified().await;

        drop(task);
        tokio::task::yield_now().await;

        assert!(dropped.load(Ordering::Acquire));
    }
}
