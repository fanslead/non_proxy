use std::{sync::Arc, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD};
use nonproxy_flow_protocol::FlowEndpoint;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use zeroize::Zeroizing;

use crate::{BoxedProxyStream, OutboundError, ProxyCredentials, ProxyEndpoint, TcpDialer};

const MAXIMUM_HEADER_BYTES: usize = 16 * 1024;

pub struct HttpConnectConnector {
    endpoint: ProxyEndpoint,
    credentials: Option<ProxyCredentials>,
    timeout: Duration,
    dialer: Arc<dyn TcpDialer>,
}

impl HttpConnectConnector {
    pub fn new(
        endpoint: ProxyEndpoint,
        credentials: Option<ProxyCredentials>,
        timeout: Duration,
        dialer: Arc<dyn TcpDialer>,
    ) -> Self {
        Self {
            endpoint,
            credentials,
            timeout,
            dialer,
        }
    }

    pub async fn connect(&self, target: &FlowEndpoint) -> Result<BoxedProxyStream, OutboundError> {
        let operation = async {
            let endpoint = FlowEndpoint::new(self.endpoint.host(), self.endpoint.port())
                .map_err(|_| OutboundError::InvalidEndpoint)?;
            let mut stream = self.dialer.connect(&endpoint).await?;
            let request = self.request(target);
            stream.write_all(request.as_slice()).await?;
            stream.flush().await?;
            let status = read_response_status(&mut stream).await?;
            if !(200..300).contains(&status) {
                return Err(OutboundError::HttpRejected(status));
            }
            Ok::<BoxedProxyStream, OutboundError>(Box::new(stream))
        };
        timeout(self.timeout, operation)
            .await
            .map_err(|_| OutboundError::ConnectTimeout)?
    }

    fn request(&self, target: &FlowEndpoint) -> Zeroizing<Vec<u8>> {
        let authority = target_authority(target);
        let mut request = Zeroizing::new(
            format!(
                "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
            )
            .into_bytes(),
        );
        if let Some(credentials) = self.credentials.as_ref() {
            let raw = Zeroizing::new(format!(
                "{}:{}",
                credentials.username(),
                credentials.password()
            ));
            let encoded = Zeroizing::new(STANDARD.encode(raw.as_bytes()));
            request.extend_from_slice(b"Proxy-Authorization: Basic ");
            request.extend_from_slice(encoded.as_bytes());
            request.extend_from_slice(b"\r\n");
        }
        request.extend_from_slice(b"\r\n");
        request
    }
}

async fn read_response_status(stream: &mut TcpStream) -> Result<u16, OutboundError> {
    let mut header = Zeroizing::new(Vec::with_capacity(512));
    while header.len() < MAXIMUM_HEADER_BYTES {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).await?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            return parse_status(&header);
        }
    }
    Err(OutboundError::HttpHeaderTooLarge)
}

fn parse_status(header: &[u8]) -> Result<u16, OutboundError> {
    let first_line_end = header
        .windows(2)
        .position(|value| value == b"\r\n")
        .ok_or(OutboundError::InvalidHttpResponse)?;
    let first_line = std::str::from_utf8(&header[..first_line_end])
        .map_err(|_| OutboundError::InvalidHttpResponse)?;
    let mut fields = first_line.split_ascii_whitespace();
    let version = fields.next().ok_or(OutboundError::InvalidHttpResponse)?;
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(OutboundError::InvalidHttpResponse);
    }
    let status = fields
        .next()
        .ok_or(OutboundError::InvalidHttpResponse)?
        .parse::<u16>()
        .map_err(|_| OutboundError::InvalidHttpResponse)?;
    Ok(status)
}

fn target_authority(target: &FlowEndpoint) -> String {
    match target {
        FlowEndpoint::Ip(address) if address.is_ipv6() => {
            format!("[{}]:{}", address.ip(), address.port())
        }
        _ => format!("{}:{}", target.host(), target.port()),
    }
}
