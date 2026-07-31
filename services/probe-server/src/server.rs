use std::{
    convert::Infallible,
    future::Future,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::Full;
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use nonproxy_exit_probe::{ExitProbeSigner, ProbeNonce};
use rustls::ServerConfig;
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet, time::timeout};
use tokio_rustls::TlsAcceptor;

use crate::error::ProbeServerError;

const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(15);
const EXIT_PATH: &str = "/v1/exit";
const HEALTH_PATH: &str = "/health";

pub async fn serve<F>(
    listener: TcpListener,
    tls: Arc<ServerConfig>,
    signer: ExitProbeSigner,
    maximum_connections: usize,
    shutdown: F,
) -> Result<(), ProbeServerError>
where
    F: Future<Output = ()>,
{
    let acceptor = TlsAcceptor::from(tls);
    let signer = Arc::new(signer);
    let limit = Arc::new(Semaphore::new(maximum_connections));
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => break,
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                let _completed = result;
            }
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let Ok(permit) = Arc::clone(&limit).try_acquire_owned() else {
                    continue;
                };
                let acceptor = acceptor.clone();
                let signer = Arc::clone(&signer);
                connections.spawn(async move {
                    let _permit = permit;
                    let Ok(Ok(tls_stream)) =
                        timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await
                    else {
                        return;
                    };
                    let service = service_fn(move |request| {
                        response(request, peer, Arc::clone(&signer))
                    });
                    let mut builder = http1::Builder::new();
                    builder.keep_alive(false);
                    let connection = builder.serve_connection(TokioIo::new(tls_stream), service);
                    let _result = timeout(CONNECTION_IDLE_TIMEOUT, connection).await;
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}

async fn response(
    request: Request<Incoming>,
    peer: SocketAddr,
    signer: Arc<ExitProbeSigner>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let response = match (request.method(), request.uri().path()) {
        (&Method::GET, HEALTH_PATH) if request.uri().query().is_none() => {
            json_response(StatusCode::OK, br#"{"status":"ok"}"#.to_vec())
        }
        (&Method::GET, EXIT_PATH) => exit_response(request.uri().query(), peer.ip(), &signer),
        _ => empty_response(StatusCode::NOT_FOUND),
    };
    Ok(response)
}

fn exit_response(
    query: Option<&str>,
    peer_ip: IpAddr,
    signer: &ExitProbeSigner,
) -> Response<Full<Bytes>> {
    let Some(nonce) = query.and_then(|value| value.strip_prefix("nonce=")) else {
        return empty_response(StatusCode::BAD_REQUEST);
    };
    if nonce.is_empty() || nonce.contains('&') {
        return empty_response(StatusCode::BAD_REQUEST);
    }
    let Ok(nonce) = ProbeNonce::from_base64(nonce) else {
        return empty_response(StatusCode::BAD_REQUEST);
    };
    let observed_ip = normalize_peer_ip(peer_ip);
    let Ok(observed_at_unix_ms) = unix_time_ms() else {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let Ok(receipt) = signer.sign(nonce, observed_ip, observed_at_unix_ms) else {
        return empty_response(StatusCode::BAD_REQUEST);
    };
    let Ok(body) = serde_json::to_vec(&receipt) else {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
    };
    json_response(StatusCode::OK, body)
}

fn normalize_peer_ip(value: IpAddr) -> IpAddr {
    match value {
        IpAddr::V6(address) => address.to_ipv4_mapped().map_or(value, IpAddr::V4),
        IpAddr::V4(_) => value,
    }
}

fn json_response(status: StatusCode, body: Vec<u8>) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    response
}

fn empty_response(status: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = status;
    response
}

fn unix_time_ms() -> Result<u64, ProbeServerError> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProbeServerError::Configuration)?
        .as_millis();
    u64::try_from(value).map_err(|_| ProbeServerError::Configuration)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use http::StatusCode;
    use http_body_util::BodyExt as _;
    use nonproxy_exit_probe::{ExitProbeReceipt, ExitProbeSigner, ExitProbeVerifier, ProbeNonce};

    use super::exit_response;

    #[test]
    fn signed_response_uses_only_the_peer_address_and_nonce() {
        let signer = signer();
        let verifier = ExitProbeVerifier::from_public_key_base64(&signer.public_key_base64());
        let Ok(verifier) = verifier else {
            panic!("测试验签器创建失败: {verifier:?}");
        };
        let nonce = nonce(9);
        let query = format!("nonce={}", nonce.to_base64());

        let response = exit_response(Some(&query), Ipv4Addr::new(8, 8, 8, 8).into(), &signer);
        let runtime = tokio::runtime::Runtime::new();
        let Ok(runtime) = runtime else {
            panic!("测试运行时创建失败: {runtime:?}");
        };
        let body = runtime.block_on(response.into_body().collect());
        assert!(body.is_ok());
        let body = body.ok().map(|value| value.to_bytes()).unwrap_or_default();
        let receipt = serde_json::from_slice::<ExitProbeReceipt>(&body);
        let Ok(receipt) = receipt else {
            panic!("测试回执解析失败: {receipt:?}");
        };
        let now = receipt.observed_at_unix_ms;

        assert!(verifier.verify(nonce, receipt, now).is_ok());
    }

    #[test]
    fn malformed_or_private_requests_do_not_receive_a_receipt() {
        let signer = signer();

        assert_eq!(
            exit_response(None, Ipv4Addr::new(8, 8, 8, 8).into(), &signer).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            exit_response(
                Some("nonce=invalid&extra=value"),
                Ipv4Addr::new(8, 8, 8, 8).into(),
                &signer,
            )
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            exit_response(
                Some(&format!("nonce={}", nonce(1).to_base64())),
                Ipv4Addr::LOCALHOST.into(),
                &signer,
            )
            .status(),
            StatusCode::BAD_REQUEST
        );
    }

    fn signer() -> ExitProbeSigner {
        match ExitProbeSigner::from_secret_bytes(&[7; 32]) {
            Ok(value) => value,
            Err(error) => panic!("测试签名器创建失败: {error}"),
        }
    }

    fn nonce(seed: u8) -> ProbeNonce {
        let encoded = URL_SAFE_NO_PAD.encode([seed; 32]);
        match ProbeNonce::from_base64(&encoded) {
            Ok(value) => value,
            Err(error) => panic!("测试 nonce 创建失败: {error}"),
        }
    }
}
