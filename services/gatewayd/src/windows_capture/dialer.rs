use std::{
    future::Future,
    io,
    net::SocketAddr,
    os::windows::io::AsRawSocket,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use nonproxy_flow_protocol::FlowEndpoint;
use nonproxy_outbound::{OutboundError, TcpDialer};
use nonproxy_windows_network::{
    AddressFamily, PhysicalInterfaceCatalog, WindowsNetworkError, bind_unicast_interface,
};
use nonproxy_windows_wfp::apply_redirect_records;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::{TcpStream, lookup_host};

pub struct RedirectTcpDialer {
    records: Arc<[u8]>,
    physical_interfaces: Option<Arc<PhysicalInterfaceCatalog>>,
    direct_path: Option<DirectPathObserver>,
}

#[derive(Clone, Default)]
pub struct DirectPathObserver {
    interface_index: Arc<AtomicU32>,
}

impl DirectPathObserver {
    #[must_use]
    pub fn interface_index(&self) -> Option<u32> {
        match self.interface_index.load(Ordering::Acquire) {
            0 => None,
            value => Some(value),
        }
    }
}

impl RedirectTcpDialer {
    pub fn new(records: &[u8]) -> Self {
        Self {
            records: Arc::from(records),
            physical_interfaces: None,
            direct_path: None,
        }
    }

    pub fn direct(
        records: &[u8],
        physical_interfaces: Arc<PhysicalInterfaceCatalog>,
    ) -> (Self, DirectPathObserver) {
        let direct_path = DirectPathObserver::default();
        (
            Self {
                records: Arc::from(records),
                physical_interfaces: Some(physical_interfaces),
                direct_path: Some(direct_path.clone()),
            },
            direct_path,
        )
    }

    async fn connect_endpoint(&self, endpoint: &FlowEndpoint) -> Result<TcpStream, OutboundError> {
        let addresses = resolve(endpoint).await?;
        let mut last_error = None;
        for address in addresses {
            match self.connect_address(address).await {
                Ok(stream) => return Ok(stream),
                Err(error) => last_error = Some(error),
            }
        }
        Err(OutboundError::Io(last_error.unwrap_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "目标没有可用地址")
        })))
    }

    async fn connect_address(&self, address: SocketAddr) -> io::Result<TcpStream> {
        let socket = Socket::new(
            Domain::for_address(address),
            Type::STREAM,
            Some(Protocol::TCP),
        )?;
        socket.set_nonblocking(true)?;
        socket.set_tcp_nodelay(true)?;
        apply_redirect_records(socket.as_raw_socket(), &self.records).map_err(io::Error::other)?;
        let bound_interface = if let Some(catalog) = &self.physical_interfaces {
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
            Some(interface_index)
        } else {
            None
        };
        if let Err(error) = socket.connect(&address.into())
            && error.kind() != io::ErrorKind::WouldBlock
        {
            return Err(error);
        }
        let standard: std::net::TcpStream = socket.into();
        let stream = TcpStream::from_std(standard)?;
        stream.writable().await?;
        if let Some(error) = stream.take_error()? {
            return Err(error);
        }
        if let (Some(interface_index), Some(observer)) = (bound_interface, &self.direct_path) {
            observer
                .interface_index
                .store(interface_index.get(), Ordering::Release);
        }
        Ok(stream)
    }
}

impl TcpDialer for RedirectTcpDialer {
    fn connect<'a>(
        &'a self,
        endpoint: &'a FlowEndpoint,
    ) -> Pin<Box<dyn Future<Output = Result<TcpStream, OutboundError>> + Send + 'a>> {
        Box::pin(self.connect_endpoint(endpoint))
    }
}

async fn resolve(endpoint: &FlowEndpoint) -> Result<Vec<SocketAddr>, OutboundError> {
    match endpoint {
        FlowEndpoint::Ip(address) => Ok(vec![*address]),
        FlowEndpoint::Domain(domain, port) => lookup_host((domain.as_ascii(), *port))
            .await
            .map(|addresses| addresses.collect())
            .map_err(OutboundError::from),
    }
}
