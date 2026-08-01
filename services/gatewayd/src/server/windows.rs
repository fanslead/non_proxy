use std::future::Future;

use nonproxy_proto::{
    control::v1::control_service_server::ControlServiceServer,
    provider::v1::provider_service_server::ProviderServiceServer,
};
use nonproxy_windows_ipc::NamedPipeIncoming;
use tokio::sync::{oneshot, watch};
use tonic::transport::Server;

use super::{
    WindowsPlatformDependencies, combine_server_results, monitor_runtime_events_until_shutdown,
};
use crate::{
    GatewayConfig, GatewayError,
    control_rpc_service::ControlRpcService,
    flow_server::{FlowConnectionHandler, FlowServer},
    provider_service::ProviderRpcService,
    runtime_identity::RuntimeIdentityGuard,
};

pub(super) async fn serve(
    config: GatewayConfig,
    platform: WindowsPlatformDependencies,
    control: ControlRpcService,
    provider: ProviderRpcService,
    flow: FlowConnectionHandler,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<oneshot::Sender<()>>,
) -> Result<(), GatewayError> {
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let transport = config.windows_transport();
    let incoming = bind_pipe(
        transport.control_pipe(),
        transport.pipe_sddl(),
        64,
        shutdown_receiver.clone(),
        "创建 Windows 控制命名管道",
    )?;
    let flow_incoming = bind_pipe(
        transport.flow_pipe(),
        transport.pipe_sddl(),
        254,
        shutdown_receiver.clone(),
        "创建 Windows 数据命名管道",
    )?;
    let runtime_gateway = platform.background.gateway;
    let capture = crate::windows_capture::WindowsCapture::start(
        platform.gateway,
        platform.credential_store,
        shutdown_receiver.clone(),
    )
    .await?;
    let _runtime_identity_guard = RuntimeIdentityGuard::create(&config)?;
    if let Some(sender) = ready {
        let _send_result = sender.send(());
    }
    let control_rpc = ControlServiceServer::new(control)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    let provider_rpc = ProviderServiceServer::new(provider)
        .max_decoding_message_size(ControlRpcService::max_message_bytes())
        .max_encoding_message_size(ControlRpcService::max_message_bytes());
    let flow_server = FlowServer::new(flow).serve(flow_incoming, shutdown_receiver.clone());
    let subscription_worker = platform
        .background
        .subscriptions
        .serve(shutdown_receiver.clone());
    let capture_server = capture.serve();
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
    tokio::pin!(capture_server);
    tokio::pin!(subscription_worker);
    tokio::pin!(shutdown);
    tokio::select! {
        () = &mut shutdown => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, flow_result, capture_result, ()) = tokio::join!(
                &mut control_server,
                &mut flow_server,
                &mut capture_server,
                &mut subscription_worker,
            );
            combine_server_results(control_result, flow_result)?;
            capture_result
        }
        control_result = &mut control_server => {
            let _send_result = shutdown_sender.send(true);
            let (flow_result, capture_result, ()) = tokio::join!(
                &mut flow_server,
                &mut capture_server,
                &mut subscription_worker,
            );
            combine_server_results(control_result, flow_result)?;
            capture_result
        }
        flow_result = &mut flow_server => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, capture_result, ()) = tokio::join!(
                &mut control_server,
                &mut capture_server,
                &mut subscription_worker,
            );
            combine_server_results(control_result, flow_result)?;
            capture_result
        }
        capture_result = &mut capture_server => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, flow_result, ()) = tokio::join!(
                &mut control_server,
                &mut flow_server,
                &mut subscription_worker,
            );
            combine_server_results(control_result, flow_result)?;
            capture_result
        }
        () = &mut subscription_worker => {
            let _send_result = shutdown_sender.send(true);
            let (control_result, flow_result, capture_result) =
                tokio::join!(&mut control_server, &mut flow_server, &mut capture_server);
            combine_server_results(control_result, flow_result)?;
            capture_result
        }
    }
}

fn bind_pipe(
    name: &str,
    sddl: &str,
    maximum_instances: usize,
    shutdown: watch::Receiver<bool>,
    operation: &'static str,
) -> Result<NamedPipeIncoming, GatewayError> {
    NamedPipeIncoming::bind(name, sddl, maximum_instances, shutdown)
        .map_err(|source| GatewayError::Io { operation, source })
}
