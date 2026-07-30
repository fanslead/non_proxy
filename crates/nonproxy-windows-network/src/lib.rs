mod selection;

#[cfg(windows)]
mod catalog;
#[cfg(windows)]
mod socket;

pub use selection::{
    AddressFamily, DefaultRouteCandidate, InterfaceCandidate, PhysicalInterfaces,
    select_physical_interfaces,
};

#[cfg(windows)]
pub use catalog::PhysicalInterfaceCatalog;
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
    #[error("Windows Socket 句柄或选项长度无效")]
    InvalidSocket,
}

impl WindowsNetworkError {
    #[cfg(windows)]
    pub(crate) const fn windows(operation: &'static str, code: u32) -> Self {
        Self::Windows { operation, code }
    }
}
