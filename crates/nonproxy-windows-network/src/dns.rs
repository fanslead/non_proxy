use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
    num::NonZeroU32,
    ptr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::ERROR_BUFFER_OVERFLOW,
    NetworkManagement::IpHelper::{
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_DNS_SERVER_ADDRESS_XP,
    },
    Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6},
};

use crate::{PhysicalInterfaceCatalog, WindowsNetworkError};

const DNS_PORT: u16 = 53;
const MAXIMUM_ADAPTER_TABLE_BYTES: usize = 4 * 1024 * 1024;
const MAXIMUM_ADAPTERS: usize = 4_096;
const MAXIMUM_DNS_SERVERS_PER_ADAPTER: usize = 64;
const DNS_CACHE_TTL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DnsUpstream {
    endpoint: SocketAddr,
    interface_index: NonZeroU32,
}

impl DnsUpstream {
    #[must_use]
    pub const fn endpoint(self) -> SocketAddr {
        self.endpoint
    }

    #[must_use]
    pub const fn interface_index(self) -> NonZeroU32 {
        self.interface_index
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhysicalDnsUpstreams {
    ipv4: Vec<DnsUpstream>,
    ipv6: Vec<DnsUpstream>,
}

impl PhysicalDnsUpstreams {
    #[must_use]
    pub fn preferred_direct(&self) -> &[DnsUpstream] {
        if self.ipv4.is_empty() {
            &self.ipv6
        } else {
            &self.ipv4
        }
    }

    #[must_use]
    pub fn all_endpoints(&self) -> Vec<SocketAddr> {
        self.ipv4
            .iter()
            .chain(&self.ipv6)
            .map(|value| value.endpoint)
            .collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ipv4.is_empty() && self.ipv6.is_empty()
    }
}

pub struct PhysicalDnsCatalog {
    interfaces: Arc<PhysicalInterfaceCatalog>,
    cached: Mutex<Option<CachedDnsUpstreams>>,
}

struct CachedDnsUpstreams {
    loaded_at: Instant,
    upstreams: PhysicalDnsUpstreams,
}

impl PhysicalDnsCatalog {
    #[must_use]
    pub const fn new(interfaces: Arc<PhysicalInterfaceCatalog>) -> Self {
        Self {
            interfaces,
            cached: Mutex::new(None),
        }
    }

    pub fn current(&self) -> Result<PhysicalDnsUpstreams, WindowsNetworkError> {
        let mut cached = self
            .cached
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(value) = cached.as_ref()
            && value.loaded_at.elapsed() < DNS_CACHE_TTL
        {
            return Ok(value.upstreams.clone());
        }
        let upstreams = self.load()?;
        *cached = Some(CachedDnsUpstreams {
            loaded_at: Instant::now(),
            upstreams: upstreams.clone(),
        });
        Ok(upstreams)
    }

    fn load(&self) -> Result<PhysicalDnsUpstreams, WindowsNetworkError> {
        let selected = self.interfaces.current()?;
        let mut size = 0_u32;
        // SAFETY: 首次调用按 API 约定使用空缓冲区查询所需长度。
        let first = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_UNSPEC),
                0,
                ptr::null(),
                ptr::null_mut(),
                &mut size,
            )
        };
        if first != ERROR_BUFFER_OVERFLOW || size == 0 {
            return Err(WindowsNetworkError::windows(
                "查询 Windows DNS 接口表大小",
                first,
            ));
        }
        let byte_length =
            usize::try_from(size).map_err(|_| WindowsNetworkError::InvalidDnsInterfaceTable)?;
        if byte_length > MAXIMUM_ADAPTER_TABLE_BYTES {
            return Err(WindowsNetworkError::InvalidDnsInterfaceTable);
        }
        let word_count = byte_length.div_ceil(size_of::<usize>());
        let mut buffer = vec![0_usize; word_count];
        // SAFETY: usize 缓冲区满足结构对齐，传入的字节长度不超过实际分配。
        let code = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_UNSPEC),
                0,
                ptr::null(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code != 0 {
            return Err(WindowsNetworkError::windows(
                "读取 Windows DNS 接口表",
                code,
            ));
        }
        let mut result = PhysicalDnsUpstreams::default();
        let mut unique = HashSet::new();
        let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        for _adapter_count in 0..MAXIMUM_ADAPTERS {
            if adapter.is_null() {
                break;
            }
            // SAFETY: 指针位于 GetAdaptersAddresses 返回的缓冲区链表中。
            let row = unsafe { &*adapter };
            // SAFETY: Anonymous 是 Windows 头文件定义的 Length/IfIndex 联合体。
            let ipv4_index = NonZeroU32::new(unsafe { row.Anonymous1.Anonymous.IfIndex });
            let ipv6_index = NonZeroU32::new(row.Ipv6IfIndex);
            if ipv4_index == selected.ipv4() {
                collect_dns(
                    row.FirstDnsServerAddress,
                    AF_INET,
                    ipv4_index,
                    &mut result.ipv4,
                    &mut unique,
                )?;
            }
            if ipv6_index == selected.ipv6() {
                collect_dns(
                    row.FirstDnsServerAddress,
                    AF_INET6,
                    ipv6_index,
                    &mut result.ipv6,
                    &mut unique,
                )?;
            }
            adapter = row.Next;
        }
        if !adapter.is_null() {
            return Err(WindowsNetworkError::InvalidDnsInterfaceTable);
        }
        if result.is_empty() {
            return Err(WindowsNetworkError::PhysicalDnsUnavailable);
        }
        Ok(result)
    }
}

fn collect_dns(
    mut current: *const IP_ADAPTER_DNS_SERVER_ADDRESS_XP,
    expected_family: u16,
    interface_index: Option<NonZeroU32>,
    output: &mut Vec<DnsUpstream>,
    unique: &mut HashSet<SocketAddr>,
) -> Result<(), WindowsNetworkError> {
    let Some(interface_index) = interface_index else {
        return Ok(());
    };
    for _server_count in 0..MAXIMUM_DNS_SERVERS_PER_ADAPTER {
        if current.is_null() {
            return Ok(());
        }
        // SAFETY: 指针来自当前 adapter 的 FirstDnsServerAddress 链表。
        let row = unsafe { &*current };
        if let Some(endpoint) = socket_address(row, expected_family)?
            && is_usable(endpoint.ip())
            && unique.insert(endpoint)
        {
            output.push(DnsUpstream {
                endpoint,
                interface_index,
            });
        }
        current = row.Next;
    }
    if current.is_null() {
        Ok(())
    } else {
        Err(WindowsNetworkError::InvalidDnsInterfaceTable)
    }
}

fn socket_address(
    row: &IP_ADAPTER_DNS_SERVER_ADDRESS_XP,
    expected_family: u16,
) -> Result<Option<SocketAddr>, WindowsNetworkError> {
    let address = row.Address;
    if address.lpSockaddr.is_null() || address.iSockaddrLength < i32::from(size_of::<u16>() as u16)
    {
        return Err(WindowsNetworkError::InvalidDnsInterfaceTable);
    }
    // SAFETY: Windows 保证 SOCKET_ADDRESS 至少包含 sa_family。
    let family = unsafe { (*address.lpSockaddr).sa_family };
    if family != expected_family {
        return Ok(None);
    }
    match family {
        AF_INET
            if usize::try_from(address.iSockaddrLength).ok() >= Some(size_of::<SOCKADDR_IN>()) =>
        {
            // SAFETY: 长度和地址族已验证为 SOCKADDR_IN。
            let value = unsafe { &*address.lpSockaddr.cast::<SOCKADDR_IN>() };
            // SAFETY: 读取 IN_ADDR 的四个网络序字节。
            let octets = unsafe { value.sin_addr.S_un.S_un_b };
            Ok(Some(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(
                    octets.s_b1,
                    octets.s_b2,
                    octets.s_b3,
                    octets.s_b4,
                )),
                dns_port(value.sin_port),
            )))
        }
        AF_INET6
            if usize::try_from(address.iSockaddrLength).ok() >= Some(size_of::<SOCKADDR_IN6>()) =>
        {
            // SAFETY: 长度和地址族已验证为 SOCKADDR_IN6。
            let value = unsafe { &*address.lpSockaddr.cast::<SOCKADDR_IN6>() };
            // SAFETY: IN6_ADDR 的 Byte 是固定 16 字节网络序地址。
            let octets = unsafe { value.sin6_addr.u.Byte };
            // SAFETY: 当前联合体按 sockaddr_in6 的 scope id 解释。
            let scope_id = unsafe { value.Anonymous.sin6_scope_id };
            Ok(Some(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(octets),
                dns_port(value.sin6_port),
                0,
                scope_id,
            ))))
        }
        _ => Err(WindowsNetworkError::InvalidDnsInterfaceTable),
    }
}

fn dns_port(network_order: u16) -> u16 {
    let port = u16::from_be(network_order);
    if port == 0 { DNS_PORT } else { port }
}

fn is_usable(address: IpAddr) -> bool {
    let synthetic = matches!(
        address,
        IpAddr::V4(address)
            if address.octets()[0] == 198 && matches!(address.octets()[1], 18 | 19)
    );
    !address.is_unspecified() && !address.is_multicast() && !address.is_loopback() && !synthetic
}
