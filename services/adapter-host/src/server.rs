use std::future::Future;

use nonproxy_local_auth::SessionCapability;

#[cfg(any(unix, windows))]
use crate::runtime_identity::RuntimeIdentityGuard;
use crate::{AdapterHostConfig, AdapterHostError, rpc_state::AdapterRpcService};

pub async fn run(config: AdapterHostConfig) -> Result<(), AdapterHostError> {
    run_with_shutdown(config, shutdown_signal()).await
}

pub async fn run_with_shutdown(
    config: AdapterHostConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdapterHostError> {
    config.prepare()?;
    serve_platform(config, shutdown).await
}

fn open_authenticated_service(
    config: &AdapterHostConfig,
) -> Result<AdapterRpcService, AdapterHostError> {
    let session =
        SessionCapability::create(config.state_directory(), config.capability_file_name())?;
    #[cfg(windows)]
    nonproxy_windows_security::protect_current_user_file(
        &config.state_directory().join(config.capability_file_name()),
    )
    .map_err(AdapterHostError::File)?;
    AdapterRpcService::open(
        config.catalog_path(),
        config.transaction_directory(),
        session,
    )
}

#[cfg(unix)]
async fn serve_platform(
    config: AdapterHostConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdapterHostError> {
    use nonproxy_proto::adapter::v1::adapter_service_server::AdapterServiceServer;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::transport::Server;

    let (listener, _guard) = crate::unix_socket::bind_private_socket(config.socket_path()).await?;
    let service = open_authenticated_service(&config)?;
    let _runtime_identity = RuntimeIdentityGuard::create(&config)?;
    let rpc = AdapterServiceServer::new(service)
        .max_decoding_message_size(AdapterRpcService::max_message_bytes())
        .max_encoding_message_size(AdapterRpcService::max_message_bytes());
    Server::builder()
        .concurrency_limit_per_connection(16)
        .add_service(rpc)
        .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
        .await?;
    Ok(())
}

#[cfg(windows)]
async fn serve_platform(
    config: AdapterHostConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdapterHostError> {
    use nonproxy_proto::adapter::v1::adapter_service_server::AdapterServiceServer;
    use nonproxy_windows_ipc::NamedPipeIncoming;
    use tokio::sync::watch;
    use tonic::transport::Server;

    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let transport = config.windows_transport();
    let incoming = NamedPipeIncoming::bind(
        transport.pipe(),
        transport.pipe_sddl(),
        16,
        shutdown_receiver,
    )
    .map_err(AdapterHostError::File)?;
    let service = open_authenticated_service(&config)?;
    let _runtime_identity = RuntimeIdentityGuard::create(&config)?;
    let rpc = AdapterServiceServer::new(service)
        .max_decoding_message_size(AdapterRpcService::max_message_bytes())
        .max_encoding_message_size(AdapterRpcService::max_message_bytes());
    let stop = async move {
        shutdown.await;
        let _send_result = shutdown_sender.send(true);
    };
    Server::builder()
        .concurrency_limit_per_connection(16)
        .add_service(rpc)
        .serve_with_incoming_shutdown(incoming, stop)
        .await?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
async fn serve_platform(
    _config: AdapterHostConfig,
    _shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdapterHostError> {
    Err(AdapterHostError::Configuration)
}

#[cfg(unix)]
async fn shutdown_signal() {
    let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
    let Ok(mut terminate) = terminate else {
        let _interrupt = tokio::signal::ctrl_c().await;
        return;
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _interrupt = tokio::signal::ctrl_c().await;
}
