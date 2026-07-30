use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::control::v1::control_service_server::ControlServiceServer;

use crate::{
    GatewayConfig, GatewayError, control_service::ControlRpcService, gateway::Gateway,
    session_capability::SessionCapability,
};

#[cfg(unix)]
use std::{
    fs,
    future::Future,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub async fn run(config: GatewayConfig) -> Result<(), GatewayError> {
    config.prepare()?;
    let gateway = Gateway::open(config.database_path(), CompileCapabilities::full()).await?;
    let session = SessionCapability::create(config.state_directory())?;
    serve_platform(config, ControlRpcService::new(gateway, session)).await
}

#[cfg(unix)]
async fn serve_platform(
    config: GatewayConfig,
    service: ControlRpcService,
) -> Result<(), GatewayError> {
    serve_unix_with_shutdown(config, service, shutdown_signal()).await
}

#[cfg(unix)]
async fn serve_unix_with_shutdown(
    config: GatewayConfig,
    service: ControlRpcService,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), GatewayError> {
    use tokio::net::UnixListener;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::transport::Server;

    prepare_socket_path(config.socket_path()).await?;
    let listener = UnixListener::bind(config.socket_path()).map_err(|source| GatewayError::Io {
        operation: "绑定控制套接字",
        source,
    })?;
    fs::set_permissions(config.socket_path(), fs::Permissions::from_mode(0o600)).map_err(
        |source| GatewayError::Io {
            operation: "限制控制套接字权限",
            source,
        },
    )?;
    let metadata =
        fs::symlink_metadata(config.socket_path()).map_err(|source| GatewayError::Io {
            operation: "读取控制套接字标识",
            source,
        })?;
    let _socket_guard = SocketGuard {
        path: config.socket_path().to_path_buf(),
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    let incoming = UnixListenerStream::new(listener);
    let rpc = ControlServiceServer::new(service)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    Server::builder()
        .concurrency_limit_per_connection(64)
        .add_service(rpc)
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await?;
    Ok(())
}

#[cfg(unix)]
async fn prepare_socket_path(path: &Path) -> Result<(), GatewayError> {
    use tokio::net::UnixStream;

    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(GatewayError::InvalidLocalPath("控制套接字不能是符号链接"))
        }
        Ok(metadata) if !metadata.file_type().is_socket() => Err(GatewayError::InvalidLocalPath(
            "控制套接字路径已被普通文件占用",
        )),
        Ok(_) => match UnixStream::connect(path).await {
            Ok(_) => Err(GatewayError::InvalidLocalPath("另一个 gatewayd 已在运行")),
            Err(_) => fs::remove_file(path).map_err(|source| GatewayError::Io {
                operation: "移除失效控制套接字",
                source,
            }),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GatewayError::Io {
            operation: "检查控制套接字路径",
            source,
        }),
    }
}

#[cfg(unix)]
struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket()
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            let _result = fs::remove_file(&self.path);
        }
    }
}

#[cfg(not(unix))]
async fn serve_platform(
    _config: GatewayConfig,
    _service: ControlRpcService,
) -> Result<(), GatewayError> {
    Err(GatewayError::InvalidLocalPath(
        "当前目标尚未实现命名管道控制传输",
    ))
}

async fn shutdown_signal() {
    let _result = tokio::signal::ctrl_c().await;
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use hyper_util::rt::TokioIo;
    use nonproxy_policy_compiler::CompileCapabilities;
    use nonproxy_proto::control::v1::{
        GetSystemStatusRequest, control_service_client::ControlServiceClient,
    };
    use nonproxy_storage::PolicyDatabase;
    use tokio::{
        net::UnixStream,
        sync::oneshot,
        time::{Duration, sleep},
    };
    use tonic::transport::Endpoint;
    use tower::service_fn;

    use super::serve_unix_with_shutdown;
    use crate::{
        GatewayConfig, control_service::ControlRpcService, gateway::Gateway,
        session_capability::SessionCapability,
    };

    #[tokio::test]
    async fn serves_status_over_private_unix_socket_and_cleans_up() {
        let directory = tempfile::tempdir();
        let Ok(directory) = directory else {
            panic!("临时目录创建失败: {directory:?}");
        };
        let socket_path = directory.path().join("gatewayd.sock");
        let config = GatewayConfig::new(directory.path(), &socket_path);
        let Ok(config) = config else {
            panic!("网关配置创建失败: {config:?}");
        };
        if let Err(error) = config.prepare() {
            panic!("网关状态目录准备失败: {error}");
        }
        let database = PolicyDatabase::open_in_memory(1);
        let Ok(database) = database else {
            panic!("测试数据库打开失败: {database:?}");
        };
        let session = SessionCapability::create(config.state_directory());
        let Ok(session) = session else {
            panic!("测试会话令牌创建失败");
        };
        let service =
            ControlRpcService::new(Gateway::new(database, CompileCapabilities::full()), session);
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server = tokio::spawn(serve_unix_with_shutdown(config, service, async move {
            let _shutdown_result = shutdown_receiver.await;
        }));

        wait_for_socket(&socket_path).await;
        assert_socket_permissions(&socket_path);
        let channel = Endpoint::from_static("http://[::]:50051")
            .connect_with_connector(service_fn({
                let socket_path = socket_path.clone();
                move |_| {
                    let socket_path = socket_path.clone();
                    async move { UnixStream::connect(socket_path).await.map(TokioIo::new) }
                }
            }))
            .await;
        let Ok(channel) = channel else {
            panic!("UDS gRPC 客户端连接失败: {channel:?}");
        };
        let response = ControlServiceClient::new(channel)
            .get_system_status(GetSystemStatusRequest {})
            .await;
        assert!(response.is_ok());

        if shutdown_sender.send(()).is_err() {
            panic!("服务器关闭信号发送失败");
        }
        let server_result = server.await;
        assert!(matches!(server_result, Ok(Ok(()))));
        assert!(!socket_path.exists());
    }

    async fn wait_for_socket(path: &Path) {
        for _attempt in 0..50 {
            if path.exists() {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("控制套接字未在限定时间内创建");
    }

    fn assert_socket_permissions(path: &Path) {
        let metadata = fs::metadata(path);
        let Ok(metadata) = metadata else {
            panic!("控制套接字元数据读取失败: {metadata:?}");
        };
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
}
