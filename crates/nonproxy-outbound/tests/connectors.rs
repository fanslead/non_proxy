use std::{future::Future, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_outbound::{
    ConnectorKind, OutboundConnector, OutboundError, ProxyCredentials, ProxyEndpoint, TcpDialer,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Mutex,
};

#[tokio::test]
async fn http_connect_sends_basic_auth_then_relays_without_losing_bytes() {
    let listener = bind_fixture().await;
    let address = listener.local_addr();
    let Ok(address) = address else {
        panic!("读取 HTTP fixture 地址失败: {address:?}");
    };
    let request_capture = Arc::new(Mutex::new(String::new()));
    let server = tokio::spawn(http_fixture(listener, Arc::clone(&request_capture)));
    let connector = connector(ConnectorKind::HttpConnect, address.port(), credentials());
    let target = target();

    let mut stream = match connector.connect_tcp(&target).await {
        Ok(value) => value,
        Err(error) => panic!("HTTP CONNECT 连接失败: {error}"),
    };
    if let Err(error) = stream.write_all(b"hello").await {
        panic!("HTTP CONNECT 写入失败: {error}");
    }
    let mut response = [0_u8; 5];
    if let Err(error) = stream.read_exact(&mut response).await {
        panic!("HTTP CONNECT 读取失败: {error}");
    }

    assert_eq!(&response, b"hello");
    let captured = request_capture.lock().await.clone();
    assert!(captured.contains("CONNECT example.com:443 HTTP/1.1"));
    assert!(captured.contains("Proxy-Authorization: Basic YWxpY2U6cHJpdmF0ZQ=="));
    if let Err(error) = server.await {
        panic!("HTTP fixture 任务失败: {error}");
    }
}

#[tokio::test]
async fn connector_uses_injected_dialer_for_proxy_control_connection() {
    let listener = bind_fixture().await;
    let address = match listener.local_addr() {
        Ok(value) => value,
        Err(error) => panic!("读取自定义拨号 fixture 地址失败: {error}"),
    };
    let request_capture = Arc::new(Mutex::new(String::new()));
    let server = tokio::spawn(http_fixture(listener, Arc::clone(&request_capture)));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let dialer: Arc<dyn TcpDialer> = Arc::new(FixedDialer {
        address,
        observed: Arc::clone(&observed),
    });
    let endpoint = match ProxyEndpoint::new("proxy.example", 8_443) {
        Ok(value) => value,
        Err(error) => panic!("创建自定义拨号代理 endpoint 失败: {error}"),
    };
    let connector = OutboundConnector::with_dialer(
        ConnectorKind::HttpConnect,
        endpoint,
        None,
        Duration::from_secs(2),
        dialer,
    );

    let mut stream = match connector.connect_tcp(&target()).await {
        Ok(value) => value,
        Err(error) => panic!("自定义拨号器 HTTP CONNECT 失败: {error}"),
    };
    if let Err(error) = stream.write_all(b"hello").await {
        panic!("自定义拨号器写入失败: {error}");
    }
    let mut response = [0_u8; 5];
    if let Err(error) = stream.read_exact(&mut response).await {
        panic!("自定义拨号器读取失败: {error}");
    }
    assert_eq!(&response, b"hello");
    let endpoints = observed.lock().await;
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].host(), "proxy.example");
    assert_eq!(endpoints[0].port(), 8_443);
    if let Err(error) = server.await {
        panic!("自定义拨号 fixture 任务失败: {error}");
    }
}

#[tokio::test]
async fn socks5_authenticates_and_relays_tcp() {
    let listener = bind_fixture().await;
    let address = listener.local_addr();
    let Ok(address) = address else {
        panic!("读取 SOCKS5 fixture 地址失败: {address:?}");
    };
    let server = tokio::spawn(socks5_fixture(listener));
    let connector = connector(ConnectorKind::Socks5, address.port(), credentials());

    let mut stream = match connector.connect_tcp(&target()).await {
        Ok(value) => value,
        Err(error) => panic!("SOCKS5 连接失败: {error}"),
    };
    if let Err(error) = stream.write_all(b"hello").await {
        panic!("SOCKS5 写入失败: {error}");
    }
    let mut response = [0_u8; 5];
    if let Err(error) = stream.read_exact(&mut response).await {
        panic!("SOCKS5 读取失败: {error}");
    }

    assert_eq!(&response, b"hello");
    if let Err(error) = server.await {
        panic!("SOCKS5 fixture 任务失败: {error}");
    }
}

#[tokio::test]
async fn socks5_udp_association_preserves_target_endpoint() {
    let listener = bind_fixture().await;
    let address = listener.local_addr();
    let Ok(address) = address else {
        panic!("读取 SOCKS5 UDP fixture 地址失败: {address:?}");
    };
    let relay = UdpSocket::bind(("127.0.0.1", 0)).await;
    let Ok(relay) = relay else {
        panic!("绑定 SOCKS5 UDP relay 失败: {relay:?}");
    };
    let server = tokio::spawn(socks5_udp_fixture(listener, relay));
    let connector = connector(ConnectorKind::Socks5, address.port(), credentials());
    let association = match connector.open_udp().await {
        Ok(value) => value,
        Err(error) => panic!("SOCKS5 UDP association 创建失败: {error}"),
    };
    let target = FlowEndpoint::new("dns.example", 53);
    let Ok(target) = target else {
        panic!("SOCKS5 UDP 测试目标创建失败: {target:?}");
    };

    if let Err(error) = association.send(&target, b"hello").await {
        panic!("SOCKS5 UDP 发送失败: {error}");
    }
    let received = association.receive().await;
    let Ok((endpoint, payload)) = received else {
        panic!("SOCKS5 UDP 接收失败: {received:?}");
    };

    assert_eq!(endpoint, target);
    assert_eq!(payload, b"hello");
    if let Err(error) = server.await {
        panic!("SOCKS5 UDP fixture 任务失败: {error}");
    }
}

#[test]
fn stored_credential_format_decodes_without_debug_exposure() {
    let credentials = ProxyCredentials::decode(b"\x01\x05aliceprivate");
    let Ok(credentials) = credentials else {
        panic!("测试凭据解码失败: {credentials:?}");
    };

    assert_eq!(credentials.username(), "alice");
    assert_eq!(credentials.password(), "private");
    assert!(!format!("{credentials:?}").contains("private"));
}

fn connector(kind: ConnectorKind, port: u16, credentials: ProxyCredentials) -> OutboundConnector {
    let endpoint = ProxyEndpoint::new("127.0.0.1", port);
    let Ok(endpoint) = endpoint else {
        panic!("测试代理 endpoint 创建失败: {endpoint:?}");
    };
    OutboundConnector::new(kind, endpoint, Some(credentials), Duration::from_secs(2))
}

fn credentials() -> ProxyCredentials {
    let credentials = ProxyCredentials::new("alice".to_owned(), "private".to_owned());
    let Ok(credentials) = credentials else {
        panic!("测试代理凭据创建失败: {credentials:?}");
    };
    credentials
}

fn target() -> FlowEndpoint {
    let target = FlowEndpoint::new("example.com", 443);
    let Ok(target) = target else {
        panic!("测试目标创建失败: {target:?}");
    };
    target
}

struct FixedDialer {
    address: SocketAddr,
    observed: Arc<Mutex<Vec<FlowEndpoint>>>,
}

impl TcpDialer for FixedDialer {
    fn connect<'a>(
        &'a self,
        endpoint: &'a FlowEndpoint,
    ) -> Pin<Box<dyn Future<Output = Result<TcpStream, OutboundError>> + Send + 'a>> {
        Box::pin(async move {
            self.observed.lock().await.push(endpoint.clone());
            TcpStream::connect(self.address)
                .await
                .map_err(OutboundError::from)
        })
    }
}

async fn bind_fixture() -> TcpListener {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await;
    let Ok(listener) = listener else {
        panic!("绑定代理 fixture 失败: {listener:?}");
    };
    listener
}

async fn http_fixture(listener: TcpListener, capture: Arc<Mutex<String>>) {
    let accepted = listener.accept().await;
    let Ok((mut stream, _)) = accepted else {
        panic!("HTTP fixture 接收失败: {accepted:?}");
    };
    let request = read_http_header(&mut stream).await;
    *capture.lock().await = String::from_utf8_lossy(&request).into_owned();
    if let Err(error) = stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
    {
        panic!("HTTP fixture 响应失败: {error}");
    }
    echo_five(&mut stream).await;
}

async fn read_http_header(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    while request.len() < 16 * 1024 {
        let mut byte = [0_u8; 1];
        if let Err(error) = stream.read_exact(&mut byte).await {
            panic!("HTTP fixture 读取失败: {error}");
        }
        request.push(byte[0]);
        if request.ends_with(b"\r\n\r\n") {
            return request;
        }
    }
    panic!("HTTP fixture 请求头超限");
}

async fn socks5_fixture(listener: TcpListener) {
    let mut stream = authenticated_socks5_stream(listener).await;
    let mut request = [0_u8; 5];
    read_exact(&mut stream, &mut request).await;
    assert_eq!(&request[..4], &[5, 1, 0, 3]);
    let domain_length = usize::from(request[4]);
    let mut target = vec![0_u8; domain_length + 2];
    read_exact(&mut stream, &mut target).await;
    assert_eq!(&target[..domain_length], b"example.com");
    write_all(&mut stream, &[5, 0, 0, 1, 127, 0, 0, 1, 0, 1]).await;
    echo_five(&mut stream).await;
}

async fn socks5_udp_fixture(listener: TcpListener, relay: UdpSocket) {
    let mut stream = authenticated_socks5_stream(listener).await;
    let mut request = [0_u8; 10];
    read_exact(&mut stream, &mut request).await;
    assert_eq!(&request[..4], &[5, 3, 0, 1]);
    let relay_address = relay.local_addr();
    let Ok(relay_address) = relay_address else {
        panic!("读取 SOCKS5 UDP relay 地址失败: {relay_address:?}");
    };
    let port = relay_address.port().to_be_bytes();
    write_all(&mut stream, &[5, 0, 0, 1, 127, 0, 0, 1, port[0], port[1]]).await;
    let mut packet = [0_u8; 512];
    let received = relay.recv_from(&mut packet).await;
    let Ok((length, peer)) = received else {
        panic!("SOCKS5 UDP relay 接收失败: {received:?}");
    };
    assert_eq!(&packet[..4], &[0, 0, 0, 3]);
    assert_eq!(packet[4] as usize, "dns.example".len());
    let sent = relay.send_to(&packet[..length], peer).await;
    if let Err(error) = sent {
        panic!("SOCKS5 UDP relay 发送失败: {error}");
    }
}

async fn authenticated_socks5_stream(listener: TcpListener) -> TcpStream {
    let accepted = listener.accept().await;
    let Ok((mut stream, _)) = accepted else {
        panic!("SOCKS5 fixture 接收失败: {accepted:?}");
    };
    let mut methods = [0_u8; 4];
    read_exact(&mut stream, &mut methods).await;
    assert_eq!(methods, [5, 2, 0, 2]);
    write_all(&mut stream, &[5, 2]).await;
    let mut auth_header = [0_u8; 2];
    read_exact(&mut stream, &mut auth_header).await;
    assert_eq!(auth_header, [1, 5]);
    let mut username = [0_u8; 5];
    read_exact(&mut stream, &mut username).await;
    let mut password_length = [0_u8; 1];
    read_exact(&mut stream, &mut password_length).await;
    let mut password = vec![0_u8; usize::from(password_length[0])];
    read_exact(&mut stream, &mut password).await;
    assert_eq!(&username, b"alice");
    assert_eq!(&password, b"private");
    write_all(&mut stream, &[1, 0]).await;
    stream
}

async fn echo_five(stream: &mut TcpStream) {
    let mut payload = [0_u8; 5];
    read_exact(stream, &mut payload).await;
    write_all(stream, &payload).await;
}

async fn read_exact(stream: &mut TcpStream, buffer: &mut [u8]) {
    if let Err(error) = stream.read_exact(buffer).await {
        panic!("代理 fixture 读取失败: {error}");
    }
}

async fn write_all(stream: &mut TcpStream, buffer: &[u8]) {
    if let Err(error) = stream.write_all(buffer).await {
        panic!("代理 fixture 写入失败: {error}");
    }
}
