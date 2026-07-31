use std::{net::SocketAddr, time::Duration};

use bytes::Bytes;
use http::{
    HeaderValue, Method, Request, StatusCode,
    header::{AUTHORIZATION, CONNECTION, CONTENT_TYPE, HOST},
};
use http_body_util::{BodyExt, Full, Limited};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use tokio::task::JoinHandle;
use tokio::{net::TcpStream, time::timeout};
use zeroize::Zeroizing;

use crate::AdapterHostError;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);
const MAXIMUM_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub(crate) async fn request(
    endpoint: SocketAddr,
    method: Method,
    uri: &'static str,
    secret: &str,
    body: &[u8],
) -> Result<(StatusCode, Vec<u8>), AdapterHostError> {
    timeout(
        CONTROL_TIMEOUT,
        request_within_deadline(endpoint, method, uri, secret, body),
    )
    .await
    .map_err(|_| AdapterHostError::ClientReloadFailed)?
}

async fn request_within_deadline(
    endpoint: SocketAddr,
    method: Method,
    uri: &'static str,
    secret: &str,
    body: &[u8],
) -> Result<(StatusCode, Vec<u8>), AdapterHostError> {
    let stream = TcpStream::connect(endpoint)
        .await
        .map_err(|_| AdapterHostError::ClientReloadFailed)?;
    let (mut sender, connection) = http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|_| AdapterHostError::ClientReloadFailed)?;
    let driver = tokio::spawn(async move {
        let _connection_result = connection.await;
    });
    let driver = ConnectionDriver::new(driver);
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(HOST, endpoint.to_string())
        .header(CONNECTION, "close")
        .header(CONTENT_TYPE, "application/json");
    if !secret.is_empty() {
        let bearer = Zeroizing::new(format!("Bearer {secret}"));
        let value = HeaderValue::from_str(&bearer)
            .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        builder = builder.header(AUTHORIZATION, value);
    }
    let request = builder
        .body(Full::new(Bytes::copy_from_slice(body)))
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|_| AdapterHostError::ClientReloadFailed)?;
    let status = response.status();
    let collected = Limited::new(response.into_body(), MAXIMUM_RESPONSE_BYTES)
        .collect()
        .await
        .map_err(|_| AdapterHostError::ClientReloadFailed)?;
    driver.stop().await;
    Ok((status, collected.to_bytes().to_vec()))
}

struct ConnectionDriver {
    task: Option<JoinHandle<()>>,
}

impl ConnectionDriver {
    fn new(task: JoinHandle<()>) -> Self {
        Self { task: Some(task) }
    }

    async fn stop(mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
            let _task_result = task.await;
        }
    }
}

impl Drop for ConnectionDriver {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
