use std::{future::Future, net::SocketAddr, pin::Pin, sync::Arc, time::Duration};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_outbound::{
    OutboundConnector, OutboundError, ProxyEndpoint, ShadowsocksCredentials, TcpDialer,
};
use shadowsocks::{
    ProxyListener, ServerConfig,
    config::ServerType,
    context::Context,
    crypto::CipherKind,
    relay::{socks5::Address, udprelay::ProxySocket},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
};

#[tokio::test]
async fn aead_relays_tcp_and_uses_the_injected_dialer() {
    let config = server_config(0);
    let listener = ProxyListener::bind(Context::new_shared(ServerType::Server), &config).await;
    let Ok(listener) = listener else {
        panic!("绑定 Shadowsocks TCP fixture 失败: {listener:?}");
    };
    let address = listener.local_addr();
    let Ok(address) = address else {
        panic!("读取 Shadowsocks TCP fixture 地址失败: {address:?}");
    };
    let server = tokio::spawn(async move {
        let accepted = listener.accept().await;
        let Ok((mut stream, _)) = accepted else {
            panic!("Shadowsocks TCP fixture 接收失败: {accepted:?}");
        };
        let target = stream.handshake().await;
        assert!(matches!(
            target,
            Ok(Address::DomainNameAddress(domain, 443)) if domain == "example.com"
        ));
        echo_five(&mut stream).await;
    });
    let observed = Arc::new(Mutex::new(Vec::new()));
    let dialer: Arc<dyn TcpDialer> = Arc::new(FixedDialer {
        address,
        observed: Arc::clone(&observed),
    });
    let endpoint = ProxyEndpoint::new("ss.example", 8_388)
        .unwrap_or_else(|error| panic!("创建 Shadowsocks endpoint 失败: {error}"));
    let connector = OutboundConnector::shadowsocks_with_dialer(
        endpoint,
        credentials(),
        Duration::from_secs(2),
        dialer,
    );

    let mut stream = connector
        .connect_tcp(&target())
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks TCP 连接失败: {error}"));
    stream
        .write_all(b"hello")
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks TCP 写入失败: {error}"));
    let mut response = [0_u8; 5];
    stream
        .read_exact(&mut response)
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks TCP 读取失败: {error}"));

    assert_eq!(&response, b"hello");
    let endpoints = observed.lock().await;
    assert_eq!(
        endpoints.as_slice(),
        [FlowEndpoint::new("ss.example", 8_388)
            .unwrap_or_else(|error| panic!("创建期望 endpoint 失败: {error}"))]
    );
    drop(endpoints);
    server
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks TCP fixture 任务失败: {error}"));
}

#[tokio::test]
async fn aead_udp_preserves_domains_and_empty_datagrams() {
    let config = server_config(0);
    let socket = ProxySocket::bind(Context::new_shared(ServerType::Server), &config).await;
    let Ok(socket) = socket else {
        panic!("绑定 Shadowsocks UDP fixture 失败: {socket:?}");
    };
    let address = socket.local_addr();
    let Ok(address) = address else {
        panic!("读取 Shadowsocks UDP fixture 地址失败: {address:?}");
    };
    let server = tokio::spawn(async move {
        for expected in [b"hello".as_slice(), b"".as_slice()] {
            let mut packet = vec![0_u8; 65_535];
            let received = socket.recv_from(&mut packet).await;
            let Ok((length, peer, target, _)) = received else {
                panic!("Shadowsocks UDP fixture 接收失败: {received:?}");
            };
            assert_eq!(&packet[..length], expected);
            assert!(matches!(
                target,
                Address::DomainNameAddress(ref domain, 53) if domain == "dns.example"
            ));
            socket
                .send_to(peer, &target, &packet[..length])
                .await
                .unwrap_or_else(|error| panic!("Shadowsocks UDP fixture 响应失败: {error}"));
        }
    });
    let endpoint = ProxyEndpoint::new("127.0.0.1", address.port())
        .unwrap_or_else(|error| panic!("创建 Shadowsocks UDP endpoint 失败: {error}"));
    let connector = OutboundConnector::shadowsocks(endpoint, credentials(), Duration::from_secs(2));
    let association = connector
        .open_udp()
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks UDP 会话创建失败: {error}"));
    let target = FlowEndpoint::new("dns.example", 53)
        .unwrap_or_else(|error| panic!("创建 Shadowsocks UDP 目标失败: {error}"));

    for expected in [b"hello".as_slice(), b"".as_slice()] {
        association
            .send(&target, expected)
            .await
            .unwrap_or_else(|error| panic!("Shadowsocks UDP 发送失败: {error}"));
        let (actual_target, payload) = association
            .receive()
            .await
            .unwrap_or_else(|error| panic!("Shadowsocks UDP 接收失败: {error}"));
        assert_eq!(actual_target, target);
        assert_eq!(payload, expected);
    }
    server
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks UDP fixture 任务失败: {error}"));
}

#[test]
fn credentials_are_versioned_redacted_and_reject_unsafe_methods() {
    let credentials = credentials();
    let encoded = credentials.encode();
    let decoded = ShadowsocksCredentials::decode(encoded.as_slice())
        .unwrap_or_else(|error| panic!("Shadowsocks 凭据解码失败: {error}"));

    assert_eq!(decoded.method_name(), "aes-256-gcm");
    assert!(!format!("{decoded:?}").contains("private"));
    assert!(ShadowsocksCredentials::new("none", "private".to_owned()).is_err());
    assert!(ShadowsocksCredentials::new("aes-256-cfb", "private".to_owned()).is_err());
    assert!(ShadowsocksCredentials::decode(b"\x01\x00private").is_err());
}

#[test]
fn every_declared_aead_method_accepts_its_required_key_shape() {
    let key_128 = "MDEyMzQ1Njc4OWFiY2RlZg==";
    let key_256 = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
    for (method, password) in [
        ("aes-128-gcm", "private"),
        ("aes-256-gcm", "private"),
        ("chacha20-ietf-poly1305", "private"),
        ("2022-blake3-aes-128-gcm", key_128),
        ("2022-blake3-aes-256-gcm", key_256),
        ("2022-blake3-chacha20-poly1305", key_256),
    ] {
        let credentials = ShadowsocksCredentials::new(method, password.to_owned())
            .unwrap_or_else(|error| panic!("{method} 凭据应当有效: {error}"));
        assert_eq!(credentials.method_name(), method);
        assert_eq!(
            ShadowsocksCredentials::decode(credentials.encode().as_slice())
                .unwrap_or_else(|error| panic!("{method} 凭据往返失败: {error}"))
                .method_name(),
            method
        );
    }

    assert!(
        ShadowsocksCredentials::new("2022-blake3-aes-256-gcm", "not-a-valid-key".to_owned())
            .is_err()
    );
}

fn credentials() -> ShadowsocksCredentials {
    ShadowsocksCredentials::new("aes-256-gcm", "private".to_owned())
        .unwrap_or_else(|error| panic!("创建 Shadowsocks 测试凭据失败: {error}"))
}

fn server_config(port: u16) -> ServerConfig {
    ServerConfig::new(("127.0.0.1", port), "private", CipherKind::AES_256_GCM)
        .unwrap_or_else(|error| panic!("创建 Shadowsocks fixture 配置失败: {error}"))
}

fn target() -> FlowEndpoint {
    FlowEndpoint::new("example.com", 443)
        .unwrap_or_else(|error| panic!("创建 Shadowsocks 测试目标失败: {error}"))
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

async fn echo_five<S>(stream: &mut S)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut payload = [0_u8; 5];
    stream
        .read_exact(&mut payload)
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks fixture 读取失败: {error}"));
    stream
        .write_all(&payload)
        .await
        .unwrap_or_else(|error| panic!("Shadowsocks fixture 写入失败: {error}"));
}
