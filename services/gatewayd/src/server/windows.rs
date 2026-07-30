use std::future::Future;

use nonproxy_proto::{
    control::v1::control_service_server::ControlServiceServer,
    provider::v1::provider_service_server::ProviderServiceServer,
};
use tokio::sync::{oneshot, watch};
use tonic::transport::Server;

use super::{combine_server_results, wait_for_shutdown};
use crate::{
    GatewayConfig, GatewayError,
    control_rpc_service::ControlRpcService,
    flow_server::{FlowConnectionHandler, FlowServer},
    provider_service::ProviderRpcService,
    runtime_identity::RuntimeIdentityGuard,
    windows_pipe::NamedPipeIncoming,
};

pub(super) async fn serve(
    config: GatewayConfig,
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
