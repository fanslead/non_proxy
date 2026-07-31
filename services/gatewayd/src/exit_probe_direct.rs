use nonproxy_flow_protocol::FlowEndpoint;
use thiserror::Error;
use tokio::net::TcpStream;

use crate::Gateway;

#[derive(Debug, Error)]
pub enum DirectExitConnectError {
    #[error("防回环系统快照尚未激活")]
    SystemSnapshotPending,
    #[cfg(windows)]
    #[error("物理直连出口不可用")]
    PhysicalInterfaceUnavailable,
    #[error("出口探针直连失败")]
    Connect,
}

pub async fn connect(
    gateway: &Gateway,
    endpoint: &FlowEndpoint,
) -> Result<TcpStream, DirectExitConnectError> {
    if !gateway.system_snapshot_ready() {
        return Err(DirectExitConnectError::SystemSnapshotPending);
    }
    connect_platform(endpoint).await
}

#[cfg(not(windows))]
async fn connect_platform(endpoint: &FlowEndpoint) -> Result<TcpStream, DirectExitConnectError> {
    TcpStream::connect((endpoint.host(), endpoint.port()))
        .await
        .map_err(|_| DirectExitConnectError::Connect)
}

#[cfg(windows)]
async fn connect_platform(endpoint: &FlowEndpoint) -> Result<TcpStream, DirectExitConnectError> {
    let host = endpoint.host();
    nonproxy_windows_network::connect_physical_tcp(&host, endpoint.port())
        .await
        .map_err(|error| match error {
            nonproxy_windows_network::PhysicalTcpError::PhysicalInterfaceUnavailable => {
                DirectExitConnectError::PhysicalInterfaceUnavailable
            }
            nonproxy_windows_network::PhysicalTcpError::Connect => DirectExitConnectError::Connect,
        })
}
