use std::{io, net::SocketAddr, os::windows::io::AsRawSocket};

use socket2::{Domain, Protocol, Socket, Type};
use thiserror::Error;
use tokio::net::{TcpStream, lookup_host};

use crate::{AddressFamily, PhysicalInterfaceCatalog, bind_unicast_interface};

#[derive(Debug, Error)]
pub enum PhysicalTcpError {
    #[error("没有可用的 Windows 物理网络接口")]
    PhysicalInterfaceUnavailable,
    #[error("Windows 物理 TCP 连接失败")]
    Connect,
}

pub async fn connect_physical_tcp(host: &str, port: u16) -> Result<TcpStream, PhysicalTcpError> {
    let addresses = lookup_host((host, port))
        .await
        .map_err(|_| PhysicalTcpError::Connect)?;
    let catalog = PhysicalInterfaceCatalog::new();
    let interfaces = catalog
        .current()
        .map_err(|_| PhysicalTcpError::PhysicalInterfaceUnavailable)?;
    for address in addresses {
        if let Ok(stream) = connect_address(address, &interfaces).await {
            return Ok(stream);
        }
    }
    Err(PhysicalTcpError::Connect)
}

async fn connect_address(
    address: SocketAddr,
    interfaces: &crate::PhysicalInterfaces,
) -> io::Result<TcpStream> {
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    socket.set_nonblocking(true)?;
    socket.set_tcp_nodelay(true)?;
    let family = if address.is_ipv4() {
        AddressFamily::Ipv4
    } else {
        AddressFamily::Ipv6
    };
    let interface = interfaces
        .for_family(family)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "没有可用的物理网络接口"))?;
    bind_unicast_interface(socket.as_raw_socket(), address.ip(), interface)
        .map_err(io::Error::other)?;
    if let Err(error) = socket.connect(&address.into())
        && error.kind() != io::ErrorKind::WouldBlock
    {
        return Err(error);
    }
    let stream = TcpStream::from_std(socket.into())?;
    stream.writable().await?;
    if let Some(error) = stream.take_error()? {
        return Err(error);
    }
    Ok(stream)
}
