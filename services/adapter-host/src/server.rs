use std::future::Future;

use nonproxy_local_auth::SessionCapability;

use crate::{
    AdapterHostConfig, AdapterHostError, rpc_state::AdapterRpcService,
    runtime_identity::RuntimeIdentityGuard,
};

pub async fn run(config: AdapterHostConfig) -> Result<(), AdapterHostError> {
    run_with_shutdown(config, shutdown_signal()).await
}

pub async fn run_with_shutdown(
    config: AdapterHostConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdapterHostError> {
    config.prepare()?;
    let session =
        SessionCapability::create(config.state_directory(), config.capability_file_name())?;
    let service = AdapterRpcService::open(
        config.catalog_path(),
        config.transaction_directory(),
        session,
    )?;
    serve_platform(config, service, shutdown).await
}

#[cfg(unix)]
async fn serve_platform(
    config: AdapterHostConfig,
    service: AdapterRpcService,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), AdapterHostError> {
    use nonproxy_proto::adapter::v1::adapter_service_server::AdapterServiceServer;
    use tokio_stream::wrappers::UnixListenerStream;
    use tonic::transport::Server;

    let (listener, _guard) = crate::unix_socket::bind_private_socket(config.socket_path()).await?;
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

#[cfg(not(unix))]
async fn serve_platform(
    _config: AdapterHostConfig,
    _service: AdapterRpcService,
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
