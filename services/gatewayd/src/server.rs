use std::sync::Arc;

use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::control::v1::control_service_server::ControlServiceServer;
use nonproxy_proto::provider::v1::provider_service_server::ProviderServiceServer;

use crate::{
    GatewayConfig, GatewayError,
    control_service::ControlRpcService,
    credential_store::{CredentialStore, OsCredentialStore},
    gateway::Gateway,
    provider_service::ProviderRpcService,
    runtime_identity::RuntimeIdentityGuard,
    session_capability::SessionCapability,
};

#[cfg(unix)]
use std::future::Future;

#[cfg(unix)]
use crate::flow_server::{FlowConnectionHandler, FlowServer};

#[cfg(unix)]
use crate::unix_socket::{SocketRole, bind_private_socket};

pub async fn run(config: GatewayConfig) -> Result<(), GatewayError> {
    config.prepare()?;
    let gateway = Gateway::open(config.database_path(), CompileCapabilities::full()).await?;
    let control_capability = SessionCapability::create_control(config.state_directory())?;
    let provider_capability = SessionCapability::create_provider(config.state_directory())?;
    let credential_store: Arc<dyn CredentialStore> = Arc::new(OsCredentialStore);
    let control = ControlRpcService::with_credential_store(
        gateway.clone(),
        control_capability,
        Arc::clone(&credential_store),
    );
    #[cfg(unix)]
    {
        let provider = ProviderRpcService::with_credential_store(
            gateway.clone(),
            provider_capability.clone(),
            Arc::clone(&credential_store),
        );
        let flow = FlowConnectionHandler::new(gateway, provider_capability, credential_store);
        serve_platform(config, control, provider, flow).await
    }
    #[cfg(not(unix))]
    {
        let provider = ProviderRpcService::with_credential_store(
            gateway,
            provider_capability,
            credential_store,
        );
        serve_platform(config, control, provider).await
    }
}

#[cfg(unix)]
async fn serve_platform(
    config: GatewayConfig,
    control: ControlRpcService,
    provider: ProviderRpcService,
    flow: FlowConnectionHandler,
) -> Result<(), GatewayError> {
    serve_unix_with_shutdown(config, control, provider, flow, shutdown_signal()).await
}

#[cfg(unix)]
async fn serve_unix_with_shutdown(
    config: GatewayConfig,
    control: ControlRpcService,
    provider: ProviderRpcService,
    flow: FlowConnectionHandler,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), GatewayError> {
    use tokio::sync::watch;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::transport::Server;

    let (control_listener, _control_socket_guard) =
        bind_private_socket(config.socket_path(), SocketRole::Control).await?;
    let (flow_listener, _flow_socket_guard) =
        bind_private_socket(config.flow_socket_path(), SocketRole::Flow).await?;
    let _runtime_identity_guard = RuntimeIdentityGuard::create(&config)?;
    let incoming = UnixListenerStream::new(control_listener);
    let control_rpc = ControlServiceServer::new(control)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    let provider_rpc = ProviderServiceServer::new(provider)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let flow_server = FlowServer::new(flow).serve(flow_listener, shutdown_receiver.clone());
    let control_server = Server::builder()
        .concurrency_limit_per_connection(64)
        .add_service(control_rpc)
        .add_service(provider_rpc)
        .serve_with_incoming_shutdown(incoming, wait_for_shutdown(shutdown_receiver));
    tokio::pin!(flow_server);
    tokio::pin!(control_server);
    tokio::pin!(shutdown);
    tokio::select! {
        () = &mut shutdown => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, flow_result) =
                tokio::join!(&mut control_server, &mut flow_server);
            combine_server_results(control_result, flow_result)
        }
        control_result = &mut control_server => {
            let _send_result = shutdown_sender.send(true);
            let flow_result = flow_server.await;
            combine_server_results(control_result, flow_result)
        }
        flow_result = &mut flow_server => {
            let _send_result = shutdown_sender.send(true);
            let control_result = control_server.await;
            combine_server_results(control_result, flow_result)
        }
    }
}

#[cfg(unix)]
async fn wait_for_shutdown(mut receiver: tokio::sync::watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    while receiver.changed().await.is_ok() {
        if *receiver.borrow() {
            return;
        }
    }
}

#[cfg(unix)]
fn combine_server_results(
    control: Result<(), tonic::transport::Error>,
    flow: Result<(), std::io::Error>,
) -> Result<(), GatewayError> {
    control?;
    flow.map_err(|source| GatewayError::Io {
        operation: "运行数据套接字",
        source,
    })
}

#[cfg(not(unix))]
async fn serve_platform(
    _config: GatewayConfig,
    _control: ControlRpcService,
    _provider: ProviderRpcService,
) -> Result<(), GatewayError> {
    Err(GatewayError::InvalidLocalPath(
        "当前目标尚未实现命名管道控制传输",
    ))
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
        if let Ok(mut terminate) = terminate {
            tokio::select! {
                _result = tokio::signal::ctrl_c() => {}
                _result = terminate.recv() => {}
            }
            return;
        }
    }
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
        GatewayConfig,
        control_service::ControlRpcService,
        credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
        flow_server::FlowConnectionHandler,
        gateway::Gateway,
        provider_service::ProviderRpcService,
        session_capability::SessionCapability,
    };

    #[tokio::test]
    async fn serves_status_over_private_unix_socket_and_cleans_up() {
        let directory = tempfile::tempdir();
        let Ok(directory) = directory else {
            panic!("临时目录创建失败: {directory:?}");
        };
        let socket_path = directory.path().join("gatewayd.sock");
        let flow_socket_path = directory.path().join("gatewayd-flow.sock");
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
        let control_capability = SessionCapability::create_control(config.state_directory());
        let provider_capability = SessionCapability::create_provider(config.state_directory());
        let (Ok(control_capability), Ok(provider_capability)) =
            (control_capability, provider_capability)
        else {
            panic!("测试控制面或 Provider 能力令牌创建失败");
        };
        let gateway = Gateway::new(database, CompileCapabilities::full());
        let control = ControlRpcService::new(gateway.clone(), control_capability);
        let credential_store: std::sync::Arc<dyn CredentialStore> =
            std::sync::Arc::new(MemoryCredentialStore::default());
        let provider = ProviderRpcService::with_credential_store(
            gateway.clone(),
            provider_capability.clone(),
            std::sync::Arc::clone(&credential_store),
        );
        let flow = FlowConnectionHandler::new(gateway, provider_capability, credential_store);
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let server = tokio::spawn(serve_unix_with_shutdown(
            config,
            control,
            provider,
            flow,
            async move {
                let _shutdown_result = shutdown_receiver.await;
            },
        ));

        wait_for_socket(&socket_path).await;
        wait_for_socket(&flow_socket_path).await;
        assert_socket_permissions(&socket_path);
        assert_socket_permissions(&flow_socket_path);
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
        assert!(!flow_socket_path.exists());
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
