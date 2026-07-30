use std::{
    collections::HashMap,
    ffi::c_void,
    net::Ipv4Addr,
    num::NonZeroU32,
    ptr,
    sync::Mutex,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    NetworkManagement::{
        IpHelper::{
            FreeMibTable, GetIfTable2, GetIpForwardTable2, GetIpInterfaceEntry,
            InitializeIpInterfaceEntry, MIB_IF_ROW2, MIB_IF_TABLE2, MIB_IPFORWARD_ROW2,
            MIB_IPFORWARD_TABLE2, MIB_IPINTERFACE_ROW,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{AF_INET, AF_INET6},
};

use crate::{
    AddressFamily, DefaultRouteCandidate, InterfaceCandidate, Ipv4RoutePrefix, PhysicalInterfaces,
    WindowsNetworkError, conflicts_with_synthetic_ipv4_pool, select_physical_interfaces,
};

const CACHE_TTL: Duration = Duration::from_secs(1);
const FLAG_HARDWARE_INTERFACE: u8 = 1 << 0;
const FLAG_FILTER_INTERFACE: u8 = 1 << 1;
const FLAG_CONNECTOR_PRESENT: u8 = 1 << 2;
const FLAG_NOT_MEDIA_CONNECTED: u8 = 1 << 4;
const FLAG_ENDPOINT_INTERFACE: u8 = 1 << 7;

pub struct PhysicalInterfaceCatalog {
    cached: Mutex<Option<CachedInterfaces>>,
}

struct CachedInterfaces {
    loaded_at: Instant,
    interfaces: PhysicalInterfaces,
}

impl PhysicalInterfaceCatalog {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cached: Mutex::new(None),
        }
    }

    pub fn current(&self) -> Result<PhysicalInterfaces, WindowsNetworkError> {
        let mut cached = self
            .cached
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(value) = cached.as_ref()
            && value.loaded_at.elapsed() < CACHE_TTL
        {
            return Ok(value.interfaces);
        }
        let interfaces = load_physical_interfaces()?;
        *cached = Some(CachedInterfaces {
            loaded_at: Instant::now(),
            interfaces,
        });
        Ok(interfaces)
    }
}

impl Default for PhysicalInterfaceCatalog {
    fn default() -> Self {
        Self::new()
    }
}

pub fn ensure_synthetic_ipv4_pool_available() -> Result<(), WindowsNetworkError> {
    let mut table = ptr::null_mut::<MIB_IPFORWARD_TABLE2>();
    // SAFETY: Windows 分配输出表，成功后由 MibTableGuard 唯一释放。
    let code = unsafe { GetIpForwardTable2(AF_INET, &mut table) };
    if code != 0 {
        return Err(WindowsNetworkError::windows(
            "检查 Windows IPv4 路由冲突",
            code,
        ));
    }
    let guard = MibTableGuard(table.cast());
    if table.is_null() {
        return Err(WindowsNetworkError::InvalidRouteTable);
    }
    // SAFETY: GetIpForwardTable2 成功返回至少含表头的有效分配。
    let count = unsafe { (*table).NumEntries };
    // SAFETY: Table 是当前 guard 持有的 Windows 分配，count 来自同一表头。
    let rows = unsafe {
        table_rows(
            ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>(),
            count,
            WindowsNetworkError::InvalidRouteTable,
        )
    }?;
    let conflicted = rows.iter().any(|row| {
        // SAFETY: GetIpForwardTable2(AF_INET) 返回 IPv4 路由，读取 Ipv4 union 成员有效。
        let octets = unsafe { row.DestinationPrefix.Prefix.Ipv4.sin_addr.S_un.S_un_b };
        conflicts_with_synthetic_ipv4_pool(Ipv4RoutePrefix {
            network: Ipv4Addr::new(octets.s_b1, octets.s_b2, octets.s_b3, octets.s_b4),
            prefix_length: row.DestinationPrefix.PrefixLength,
        })
    });
    drop(guard);
    if conflicted {
        Err(WindowsNetworkError::SyntheticIpv4RouteConflict)
    } else {
        Ok(())
    }
}

fn load_physical_interfaces() -> Result<PhysicalInterfaces, WindowsNetworkError> {
    let interfaces = load_interfaces()?;
    let mut routes = load_routes(AF_INET, AddressFamily::Ipv4)?;
    routes.extend(load_routes(AF_INET6, AddressFamily::Ipv6)?);
    Ok(select_physical_interfaces(&interfaces, &routes))
}

fn load_interfaces() -> Result<Vec<InterfaceCandidate>, WindowsNetworkError> {
    let mut table = ptr::null_mut::<MIB_IF_TABLE2>();
    // SAFETY: Windows 分配输出表，成功后由 MibTableGuard 唯一释放。
    let code = unsafe { GetIfTable2(&mut table) };
    if code != 0 {
        return Err(WindowsNetworkError::windows("读取 Windows 接口表", code));
    }
    let guard = MibTableGuard(table.cast());
    if table.is_null() {
        return Err(WindowsNetworkError::InvalidInterfaceTable);
    }
    // SAFETY: GetIfTable2 成功返回至少含表头的有效分配。
    let count = unsafe { (*table).NumEntries };
    // SAFETY: Table 是当前 guard 持有的 Windows 分配，count 来自同一表头。
    let rows = unsafe {
        table_rows(
            // SAFETY: Table 是可变长数组首元素，count 由 Windows 返回。
            ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>(),
            count,
            WindowsNetworkError::InvalidInterfaceTable,
        )
    }?;
    let result = rows.iter().filter_map(interface_candidate).collect();
    drop(guard);
    Ok(result)
}

fn interface_candidate(row: &MIB_IF_ROW2) -> Option<InterfaceCandidate> {
    let index = NonZeroU32::new(row.InterfaceIndex)?;
    let flags = row.InterfaceAndOperStatusFlags._bitfield;
    Some(InterfaceCandidate {
        index,
        operational: row.OperStatus == IfOperStatusUp,
        hardware: flags & FLAG_HARDWARE_INTERFACE != 0,
        filter: flags & FLAG_FILTER_INTERFACE != 0,
        connector_present: flags & FLAG_CONNECTOR_PRESENT != 0,
        media_connected: flags & FLAG_NOT_MEDIA_CONNECTED == 0,
        endpoint: flags & FLAG_ENDPOINT_INTERFACE != 0,
        interface_type: row.Type,
        transmit_link_speed: row.TransmitLinkSpeed,
    })
}

fn load_routes(
    family: u16,
    mapped_family: AddressFamily,
) -> Result<Vec<DefaultRouteCandidate>, WindowsNetworkError> {
    let mut table = ptr::null_mut::<MIB_IPFORWARD_TABLE2>();
    // SAFETY: Windows 分配输出表，成功后由 MibTableGuard 唯一释放。
    let code = unsafe { GetIpForwardTable2(family, &mut table) };
    if code != 0 {
        return Err(WindowsNetworkError::windows("读取 Windows 路由表", code));
    }
    let guard = MibTableGuard(table.cast());
    if table.is_null() {
        return Err(WindowsNetworkError::InvalidRouteTable);
    }
    // SAFETY: GetIpForwardTable2 成功返回至少含表头的有效分配。
    let count = unsafe { (*table).NumEntries };
    // SAFETY: Table 是当前 guard 持有的 Windows 分配，count 来自同一表头。
    let rows = unsafe {
        table_rows(
            // SAFETY: Table 是可变长数组首元素，count 由 Windows 返回。
            ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>(),
            count,
            WindowsNetworkError::InvalidRouteTable,
        )
    }?;
    let mut best_by_interface = HashMap::<NonZeroU32, u32>::new();
    for row in rows {
        if row.DestinationPrefix.PrefixLength != 0 || row.Loopback {
            continue;
        }
        let Some(index) = NonZeroU32::new(row.InterfaceIndex) else {
            continue;
        };
        let Some(interface_metric) = load_interface_metric(family, index) else {
            continue;
        };
        let total_metric = row.Metric.saturating_add(interface_metric);
        best_by_interface
            .entry(index)
            .and_modify(|metric| *metric = (*metric).min(total_metric))
            .or_insert(total_metric);
    }
    drop(guard);
    Ok(best_by_interface
        .into_iter()
        .map(|(interface_index, metric)| DefaultRouteCandidate {
            interface_index,
            family: mapped_family,
            metric,
        })
        .collect())
}

fn load_interface_metric(family: u16, index: NonZeroU32) -> Option<u32> {
    let mut row = MIB_IPINTERFACE_ROW::default();
    // SAFETY: row 指向可写的完整结构，初始化函数不保留指针。
    unsafe { InitializeIpInterfaceEntry(&mut row) };
    row.Family = family;
    row.InterfaceIndex = index.get();
    // SAFETY: row 已按官方要求初始化并设置地址族和接口索引。
    let code = unsafe { GetIpInterfaceEntry(&mut row) };
    (code == 0 && row.Connected && !row.DisableDefaultRoutes).then_some(row.Metric)
}

unsafe fn table_rows<'a, T>(
    first: *const T,
    count: u32,
    invalid: WindowsNetworkError,
) -> Result<&'a [T], WindowsNetworkError> {
    let count = usize::try_from(count).map_err(|_| invalid)?;
    if count > isize::MAX as usize / size_of::<T>() {
        return Err(invalid);
    }
    // SAFETY: 安全契约要求调用方提供 Windows 表首元素和同一分配中的条目数。
    Ok(unsafe { std::slice::from_raw_parts(first, count) })
}

struct MibTableGuard(*const c_void);

impl Drop for MibTableGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: 指针来自 IP Helper 表 API，且本 guard 只释放一次。
            unsafe { FreeMibTable(self.0) };
        }
    }
}
