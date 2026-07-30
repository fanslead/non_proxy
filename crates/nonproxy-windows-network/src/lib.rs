mod selection;

#[cfg(windows)]
mod catalog;
#[cfg(windows)]
mod dns;
#[cfg(windows)]
mod dns_probe;
#[cfg(windows)]
mod socket;

pub use selection::{
    AddressFamily, DefaultRouteCandidate, InterfaceCandidate, Ipv4RoutePrefix, PhysicalInterfaces,
    conflicts_with_synthetic_ipv4_pool, select_physical_interfaces,
};

#[cfg(windows)]
pub use catalog::{PhysicalInterfaceCatalog, ensure_synthetic_ipv4_pool_available};
#[cfg(windows)]
pub use dns::{DnsUpstream, PhysicalDnsCatalog, PhysicalDnsUpstreams};
#[cfg(windows)]
pub use dns_probe::verify_system_dns_probe;
#[cfg(windows)]
pub use socket::bind_unicast_interface;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum WindowsNetworkError {
    #[error("{operation}失败，Windows 错误码 {code:#010x}")]
    Windows { operation: &'static str, code: u32 },
    #[error("没有可用的 Windows {family} 物理网络接口")]
    PhysicalInterfaceUnavailable { family: &'static str },
    #[error("Windows 网络接口表数据无效")]
    InvalidInterfaceTable,
    #[error("Windows 路由表数据无效")]
    InvalidRouteTable,
    #[error("Windows DNS 接口数据无效")]
    InvalidDnsInterfaceTable,
    #[error("198.18.0.0/15 合成 IPv4 地址池与现有非默认路由冲突")]
    SyntheticIpv4RouteConflict,
    #[error("选中的 Windows 物理接口没有可用 DNS 上游")]
    PhysicalDnsUnavailable,
    #[error("Windows Socket 句柄或选项长度无效")]
    InvalidSocket,
}

impl WindowsNetworkError {
    #[cfg(windows)]
    pub(crate) const fn windows(operation: &'static str, code: u32) -> Self {
        Self::Windows { operation, code }
    }
}
