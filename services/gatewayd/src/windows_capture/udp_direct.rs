use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    os::windows::io::AsRawSocket,
    sync::Arc,
};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_windows_network::{
    AddressFamily, PhysicalInterfaceCatalog, WindowsNetworkError, bind_unicast_interface,
};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::net::lookup_host;

use crate::GatewayError;

use super::direct_dns::WindowsDirectDomainResolver;

pub struct DirectUdpPath {
    socket: UdpSocket,
    interface_index: u32,
}

impl DirectUdpPath {
    pub fn into_parts(self) -> (UdpSocket, u32) {
        (self.socket, self.interface_index)
    }
}

pub async fn connect_direct_udp(
    target: &FlowEndpoint,
    preferred_family: IpAddr,
    resolver: &WindowsDirectDomainResolver,
    snapshot_version: u64,
    physical_interfaces: Arc<PhysicalInterfaceCatalog>,
) -> Result<DirectUdpPath, GatewayError> {
    let mut addresses = resolve(target, resolver, snapshot_version).await?;
    addresses.sort_by_key(|address| address.is_ipv4() != preferred_family.is_ipv4());
    let mut last_error = None;
    for address in addresses {
        match connect_address(address, &physical_interfaces).await {
            Ok(path) => return Ok(path),
            Err(error) => last_error = Some(error),
        }
    }
    Err(GatewayError::WindowsDataPlane(last_error.map_or_else(
        || "DIRECT UDP 没有可用地址".to_owned(),
        |error| error.to_string(),
    )))
}

pub async fn connect_system_udp(
    target: &FlowEndpoint,
    preferred_family: IpAddr,
) -> Result<UdpSocket, GatewayError> {
    let mut addresses = resolve_system(target).await?;
    addresses.sort_by_key(|address| address.is_ipv4() != preferred_family.is_ipv4());
    let mut last_error = None;
    for address in addresses {
        match UdpSocket::bind(unspecified(address.ip())).await {
            Ok(socket) => match socket.connect(address).await {
                Ok(()) => return Ok(socket),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(error),
        }
    }
    Err(GatewayError::WindowsDataPlane(last_error.map_or_else(
        || "SYSTEM UDP 没有可用地址".to_owned(),
        |error| error.to_string(),
    )))
}

async fn resolve_system(target: &FlowEndpoint) -> Result<Vec<SocketAddr>, GatewayError> {
    match target {
        FlowEndpoint::Ip(address) => Ok(vec![*address]),
        FlowEndpoint::Domain(domain, port) => lookup_host((domain.as_ascii(), *port))
            .await
            .map(|addresses| addresses.collect())
            .map_err(|error| GatewayError::Io {
                operation: "解析 SYSTEM UDP 目标",
                source: error,
            }),
    }
}

async fn resolve(
    target: &FlowEndpoint,
    resolver: &WindowsDirectDomainResolver,
    snapshot_version: u64,
) -> Result<Vec<SocketAddr>, GatewayError> {
    match target {
        FlowEndpoint::Ip(address) => Ok(vec![*address]),
        FlowEndpoint::Domain(domain, port) => {
            resolver.resolve(domain, *port, snapshot_version).await
        }
    }
}

async fn connect_address(
    address: SocketAddr,
    catalog: &PhysicalInterfaceCatalog,
) -> io::Result<DirectUdpPath> {
    let socket = Socket::new(
        Domain::for_address(address),
        Type::DGRAM,
        Some(Protocol::UDP),
    )?;
    socket.set_nonblocking(true)?;
    let interfaces = catalog.current().map_err(io::Error::other)?;
    let family = if address.is_ipv4() {
        AddressFamily::Ipv4
    } else {
        AddressFamily::Ipv6
    };
    let family_label = if address.is_ipv4() { "IPv4" } else { "IPv6" };
    let interface_index = interfaces
        .for_family(family)
        .ok_or(WindowsNetworkError::PhysicalInterfaceUnavailable {
            family: family_label,
        })
        .map_err(io::Error::other)?;
    bind_unicast_interface(socket.as_raw_socket(), address.ip(), interface_index)
        .map_err(io::Error::other)?;
    socket.bind(&unspecified(address.ip()).into())?;
    if let Err(error) = socket.connect(&address.into())
        && error.kind() != io::ErrorKind::WouldBlock
    {
        return Err(error);
    }
    let standard: std::net::UdpSocket = socket.into();
    let socket = UdpSocket::from_std(standard)?;
    socket.writable().await?;
    if let Some(error) = socket.take_error()? {
        return Err(error);
    }
    Ok(DirectUdpPath {
        socket,
        interface_index: interface_index.get(),
    })
}

const fn unspecified(address: IpAddr) -> SocketAddr {
    match address {
        IpAddr::V4(_) => SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        IpAddr::V6(_) => SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
    }
}
