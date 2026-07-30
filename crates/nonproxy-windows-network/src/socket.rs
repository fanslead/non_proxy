use std::{net::IpAddr, num::NonZeroU32, os::windows::io::RawSocket};

use windows_sys::Win32::Networking::WinSock::{
    IP_UNICAST_IF, IPPROTO_IP, IPPROTO_IPV6, IPV6_UNICAST_IF, SOCKET_ERROR, WSAGetLastError,
    setsockopt,
};

use crate::WindowsNetworkError;

pub fn bind_unicast_interface(
    socket: RawSocket,
    address: IpAddr,
    interface_index: NonZeroU32,
) -> Result<(), WindowsNetworkError> {
    let socket = usize::try_from(socket).map_err(|_| WindowsNetworkError::InvalidSocket)?;
    let (level, option, value) = match address {
        IpAddr::V4(_) => (IPPROTO_IP, IP_UNICAST_IF, interface_index.get().to_be()),
        IpAddr::V6(_) => (IPPROTO_IPV6, IPV6_UNICAST_IF, interface_index.get()),
    };
    let length = i32::try_from(size_of::<u32>()).map_err(|_| WindowsNetworkError::InvalidSocket)?;
    // SAFETY: socket 由调用方保证有效；value 是固定四字节输入且同步调用。
    let result = unsafe {
        setsockopt(
            socket,
            level,
            option,
            std::ptr::from_ref(&value).cast(),
            length,
        )
    };
    if result == SOCKET_ERROR {
        // SAFETY: WSAGetLastError 无前置条件，紧随失败的 setsockopt。
        let code = unsafe { WSAGetLastError() };
        return Err(WindowsNetworkError::windows(
            "绑定 Windows 物理网络接口",
            u32::from_ne_bytes(code.to_ne_bytes()),
        ));
    }
    Ok(())
}
