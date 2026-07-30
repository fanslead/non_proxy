use std::{ffi::c_void, net::Ipv4Addr, ptr};

use windows_sys::Win32::NetworkManagement::Dns::{
    DNS_QUERY_BYPASS_CACHE, DNS_QUERY_NO_HOSTS_FILE, DNS_QUERY_NO_MULTICAST,
    DNS_QUERY_TREAT_AS_FQDN, DNS_RECORDA, DNS_TYPE_A, DnsFree, DnsFreeRecordList, DnsQuery_W,
};

use crate::WindowsNetworkError;

const MAXIMUM_PROBE_RECORDS: usize = 64;

pub fn verify_system_dns_probe(
    domain: &str,
    expected: Ipv4Addr,
) -> Result<(), WindowsNetworkError> {
    if domain.is_empty() || domain.len() > 253 || domain.contains('\0') {
        return Err(WindowsNetworkError::InvalidDnsInterfaceTable);
    }
    let mut wide = domain.encode_utf16().collect::<Vec<_>>();
    wide.push(0);
    let mut records = ptr::null_mut::<DNS_RECORDA>();
    let options = DNS_QUERY_BYPASS_CACHE
        | DNS_QUERY_NO_HOSTS_FILE
        | DNS_QUERY_NO_MULTICAST
        | DNS_QUERY_TREAT_AS_FQDN;
    // SAFETY: wide 以 NUL 结尾，输出指针由 DNS API 分配并由 guard 释放。
    let code = unsafe {
        DnsQuery_W(
            wide.as_ptr(),
            DNS_TYPE_A,
            options,
            ptr::null_mut(),
            &mut records,
            ptr::null_mut(),
        )
    };
    if code != 0 {
        return Err(WindowsNetworkError::windows(
            "验证 Windows 系统 DNS 接管",
            code,
        ));
    }
    let guard = DnsRecordGuard(records);
    let expected = expected.octets();
    let mut current = records;
    for _record_count in 0..MAXIMUM_PROBE_RECORDS {
        if current.is_null() {
            break;
        }
        // SAFETY: current 位于 DnsQuery_W 返回并由 guard 持有的记录链表中。
        let record = unsafe { &*current };
        if record.wType == DNS_TYPE_A {
            // SAFETY: wType 已确认 A，联合体可按 DNS_A_DATA 读取。
            let address = unsafe { record.Data.A.IpAddress }.to_ne_bytes();
            if address == expected {
                drop(guard);
                return Ok(());
            }
        }
        current = record.pNext;
    }
    drop(guard);
    Err(WindowsNetworkError::PhysicalDnsUnavailable)
}

struct DnsRecordGuard(*const DNS_RECORDA);

impl Drop for DnsRecordGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: 指针来自 DnsQuery_W，DnsFreeRecordList 是匹配的释放方式。
            unsafe { DnsFree(self.0.cast::<c_void>(), DnsFreeRecordList) };
        }
    }
}
