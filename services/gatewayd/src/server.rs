use std::{future::Future, sync::Arc};

use nonproxy_policy_compiler::CompileCapabilities;
#[cfg(unix)]
use nonproxy_proto::control::v1::control_service_server::ControlServiceServer;
#[cfg(unix)]
use nonproxy_proto::provider::v1::provider_service_server::ProviderServiceServer;

use crate::{
    GatewayConfig, GatewayError,
    control_rpc_service::ControlRpcService,
    credential_store::{CredentialStore, OsCredentialStore},
    gateway::Gateway,
    outbound_health_scheduler::OutboundHealthScheduler,
    provider_service::ProviderRpcService,
    session_capability::SessionCapability,
    subscription_scheduler::SubscriptionScheduler,
    system_policies::SystemPolicyConfig,
};

#[cfg(any(unix, windows))]
use crate::flow_server::FlowConnectionHandler;
#[cfg(unix)]
use crate::{flow_server::FlowServer, runtime_identity::RuntimeIdentityGuard};

#[cfg(unix)]
use crate::unix_socket::{SocketRole, bind_private_socket};

pub async fn run(config: GatewayConfig) -> Result<(), GatewayError> {
    run_with_lifecycle(config, shutdown_signal(), None).await
}

#[cfg(windows)]
pub(crate) async fn run_windows_service(
    config: GatewayConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: tokio::sync::oneshot::Sender<()>,
) -> Result<(), GatewayError> {
    config.windows_transport().require_production_security()?;
    run_with_lifecycle(config, shutdown, Some(ready)).await
}

async fn run_with_lifecycle(
    config: GatewayConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), GatewayError> {
    config.prepare()?;
    let system_policy_config =
        SystemPolicyConfig::new(config.macos_team_identifier().map(str::to_owned))?;
    let gateway = Gateway::open_with_system_policy(
        config.database_path(),
        CompileCapabilities::full(),
        system_policy_config,
    )
    .await?;
    gateway.reconcile_required_system_snapshot().await?;
    let control_capability = SessionCapability::create_control(config.state_directory())?;
    let provider_capability = SessionCapability::create_provider(config.state_directory())?;
    let credential_store: Arc<dyn CredentialStore> = Arc::new(OsCredentialStore);
    let exit_probe_client = config
        .exit_probe()
        .map(crate::config::ExitProbeConfig::client)
        .transpose()?;
    let diagnostics_directory = config.state_directory().join("diagnostics");
    let control = ControlRpcService::with_credential_store(
        gateway.clone(),
        control_capability,
        Arc::clone(&credential_store),
    )
    .with_exit_probe_client(exit_probe_client)
    .with_diagnostics_directory(diagnostics_directory);
    #[cfg(any(unix, windows))]
    let background = BackgroundServices {
        gateway: gateway.clone(),
        health: OutboundHealthScheduler::new(gateway.clone(), Arc::clone(&credential_store)),
        subscriptions: SubscriptionScheduler::new(
            gateway.clone(),
            control.subscription_service.clone(),
        ),
    };
    #[cfg(any(unix, windows))]
    {
        let provider = ProviderRpcService::with_credential_store(
            gateway.clone(),
            provider_capability.clone(),
            Arc::clone(&credential_store),
        );
        let flow = FlowConnectionHandler::new(
            gateway.clone(),
            provider_capability,
            Arc::clone(&credential_store),
        );
        #[cfg(windows)]
        {
            let platform = WindowsPlatformDependencies {
                gateway,
                credential_store,
                background,
            };
            serve_platform(config, platform, control, provider, flow, shutdown, ready).await
        }
        #[cfg(unix)]
        {
            serve_platform(config, background, control, provider, flow, shutdown, ready).await
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let provider = ProviderRpcService::with_credential_store(
            gateway,
            provider_capability,
            credential_store,
        );
        serve_platform(config, control, provider, shutdown, ready).await
    }
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
struct WindowsPlatformDependencies {
    gateway: Gateway,
    credential_store: Arc<dyn CredentialStore>,
    background: BackgroundServices,
}

#[cfg(any(unix, windows))]
struct BackgroundServices {
    gateway: Gateway,
    health: OutboundHealthScheduler,
    subscriptions: SubscriptionScheduler,
}

#[cfg(any(unix, windows))]
impl BackgroundServices {
    async fn serve(self, shutdown: tokio::sync::watch::Receiver<bool>) {
        let subscriptions = self.subscriptions.serve(shutdown.clone());
        let health = self.health.serve(shutdown);
        tokio::join!(subscriptions, health);
    }
}

#[cfg(unix)]
async fn serve_platform(
    config: GatewayConfig,
    background: BackgroundServices,
    control: ControlRpcService,
    provider: ProviderRpcService,
    flow: FlowConnectionHandler,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), GatewayError> {
    serve_unix_with_shutdown(config, background, control, provider, flow, shutdown, ready).await
}

#[cfg(windows)]
async fn serve_platform(
    config: GatewayConfig,
    platform: WindowsPlatformDependencies,
    control: ControlRpcService,
    provider: ProviderRpcService,
    flow: FlowConnectionHandler,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), GatewayError> {
    windows::serve(config, platform, control, provider, flow, shutdown, ready).await
}

#[cfg(unix)]
async fn serve_unix_with_shutdown(
    config: GatewayConfig,
    background: BackgroundServices,
    control: ControlRpcService,
    provider: ProviderRpcService,
    flow: FlowConnectionHandler,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), GatewayError> {
    use tokio::sync::watch;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::transport::Server;

    let (control_listener, _control_socket_guard) =
        bind_private_socket(config.socket_path(), SocketRole::Control).await?;
    let (flow_listener, _flow_socket_guard) =
        bind_private_socket(config.flow_socket_path(), SocketRole::Flow).await?;
    let _runtime_identity_guard = RuntimeIdentityGuard::create(&config)?;
    if let Some(sender) = ready {
        let _send_result = sender.send(());
    }
    let incoming = UnixListenerStream::new(control_listener);
    let control_rpc = ControlServiceServer::new(control)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    let provider_rpc = ProviderServiceServer::new(provider)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let flow_server = FlowServer::new(flow).serve(
        UnixListenerStream::new(flow_listener),
        shutdown_receiver.clone(),
    );
    let runtime_gateway = background.gateway.clone();
    let background_worker = background.serve(shutdown_receiver.clone());
    let control_server = Server::builder()
        .concurrency_limit_per_connection(64)
        .add_service(control_rpc)
        .add_service(provider_rpc)
        .serve_with_incoming_shutdown(
            incoming,
            monitor_runtime_events_until_shutdown(shutdown_receiver, runtime_gateway),
        );
    tokio::pin!(flow_server);
    tokio::pin!(control_server);
    tokio::pin!(background_worker);
    tokio::pin!(shutdown);
    tokio::select! {
        () = &mut shutdown => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, flow_result, ()) =
                tokio::join!(&mut control_server, &mut flow_server, &mut background_worker);
            combine_server_results(control_result, flow_result)
        }
        control_result = &mut control_server => {
            let _send_result = shutdown_sender.send(true);
            let (flow_result, ()) = tokio::join!(&mut flow_server, &mut background_worker);
            combine_server_results(control_result, flow_result)
        }
        flow_result = &mut flow_server => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, ()) =
                tokio::join!(&mut control_server, &mut background_worker);
            combine_server_results(control_result, flow_result)
        }
        () = &mut background_worker => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, flow_result) =
                tokio::join!(&mut control_server, &mut flow_server);
            combine_server_results(control_result, flow_result)
        }
    }
}

#[cfg(any(unix, windows))]
async fn monitor_runtime_events_until_shutdown(
    mut receiver: tokio::sync::watch::Receiver<bool>,
    gateway: Gateway,
) {
    use std::time::Duration;

    use tokio::time::{MissedTickBehavior, interval};

    let mut ticker = interval(Duration::from_secs(5));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            changed = receiver.changed() => {
                if changed.is_err() || *receiver.borrow() {
                    return;
                }
            }
            _ = ticker.tick() => {
                // RPC 状态仍是权威来源；瞬时读取或广播失败留给下一次巡检重试。
                let _ = gateway.publish_runtime_events().await;
            }
        }
    }
}

#[cfg(any(unix, windows))]
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

#[cfg(not(any(unix, windows)))]
async fn serve_platform(
    _config: GatewayConfig,
    _control: ControlRpcService,
    _provider: ProviderRpcService,
    _shutdown: impl Future<Output = ()> + Send + 'static,
    _ready: Option<tokio::sync::oneshot::Sender<()>>,
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
        control_rpc_service::ControlRpcService,
        credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
        flow_server::FlowConnectionHandler,
        gateway::Gateway,
        outbound_health_scheduler::OutboundHealthScheduler,
        provider_service::ProviderRpcService,
        session_capability::SessionCapability,
        subscription_scheduler::SubscriptionScheduler,
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
        let flow = FlowConnectionHandler::new(
            gateway.clone(),
            provider_capability,
            std::sync::Arc::clone(&credential_store),
        );
        let background = super::BackgroundServices {
            gateway: gateway.clone(),
            health: OutboundHealthScheduler::new(
                gateway.clone(),
                std::sync::Arc::clone(&credential_store),
            ),
            subscriptions: SubscriptionScheduler::new(
                gateway.clone(),
                control.subscription_service.clone(),
            ),
        };
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (ready_sender, ready_receiver) = oneshot::channel();
        let server = tokio::spawn(serve_unix_with_shutdown(
            config,
            background,
            control,
            provider,
            flow,
            async move {
                let _shutdown_result = shutdown_receiver.await;
            },
            Some(ready_sender),
        ));

        let ready_result = tokio::time::timeout(Duration::from_secs(1), ready_receiver).await;
        assert!(matches!(ready_result, Ok(Ok(()))));
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
