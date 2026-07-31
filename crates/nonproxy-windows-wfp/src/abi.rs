use crate::WindowsWfpError;

pub const CONFIG_MAGIC: u32 = u32::from_le_bytes(*b"NPWF");
pub const CONFIG_VERSION: u16 = 3;
pub const CONFIG_SIZE: u16 = 40;
pub const CONFIG_FLAG_DNS_REDIRECT: u32 = 1;
pub const CONFIG_FLAG_TCP_REDIRECT: u32 = 1 << 1;
pub const CONFIG_FLAG_UDP_DIVERT: u32 = 1 << 2;

pub const STATUS_MAGIC: u32 = u32::from_le_bytes(*b"NPWS");
pub const STATUS_VERSION: u16 = 2;
pub const STATUS_SIZE: u16 = 72;

pub const REDIRECT_CONTEXT_MAGIC: u32 = u32::from_le_bytes(*b"NPWC");
pub const REDIRECT_CONTEXT_VERSION: u16 = 1;
pub const REDIRECT_CONTEXT_HEADER_SIZE: u16 = 288;
pub const MAX_APP_ID_BYTES: usize = 4_096;

const FILE_DEVICE_NETWORK: u32 = 0x12;
const METHOD_BUFFERED: u32 = 0;
const FILE_READ_DATA: u32 = 1;
const FILE_WRITE_DATA: u32 = 2;

pub const IOCTL_APPLY_CONFIG: u32 =
    ctl_code(FILE_DEVICE_NETWORK, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA);
pub const IOCTL_QUERY_STATUS: u32 =
    ctl_code(FILE_DEVICE_NETWORK, 0x802, METHOD_BUFFERED, FILE_READ_DATA);
pub const IOCTL_RECEIVE_UDP: u32 =
    ctl_code(FILE_DEVICE_NETWORK, 0x803, METHOD_BUFFERED, FILE_READ_DATA);
pub const IOCTL_INJECT_UDP: u32 =
    ctl_code(FILE_DEVICE_NETWORK, 0x804, METHOD_BUFFERED, FILE_WRITE_DATA);

const fn ctl_code(device: u32, function: u32, method: u32, access: u32) -> u32 {
    (device << 16) | (access << 14) | (function << 2) | method
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct WfpConfig {
    pub magic: u32,
    pub version: u16,
    pub size: u16,
    pub generation: u64,
    pub proxy_process_id: u64,
    pub ipv4_proxy_port_network_order: u16,
    pub ipv6_proxy_port_network_order: u16,
    pub ipv4_dns_port_network_order: u16,
    pub ipv6_dns_port_network_order: u16,
    pub flags: u32,
    pub reserved: u32,
}

impl WfpConfig {
    #[must_use]
    pub const fn disabled(generation: u64) -> Self {
        Self {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            size: CONFIG_SIZE,
            generation,
            proxy_process_id: 0,
            ipv4_proxy_port_network_order: 0,
            ipv6_proxy_port_network_order: 0,
            ipv4_dns_port_network_order: 0,
            ipv6_dns_port_network_order: 0,
            flags: 0,
            reserved: 0,
        }
    }

    #[must_use]
    pub const fn dns_only(
        generation: u64,
        proxy_process_id: u64,
        ipv4_dns_port_network_order: u16,
        ipv6_dns_port_network_order: u16,
    ) -> Self {
        Self {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            size: CONFIG_SIZE,
            generation,
            proxy_process_id,
            ipv4_proxy_port_network_order: 0,
            ipv6_proxy_port_network_order: 0,
            ipv4_dns_port_network_order,
            ipv6_dns_port_network_order,
            flags: CONFIG_FLAG_DNS_REDIRECT,
            reserved: 0,
        }
    }

    #[must_use]
    pub const fn enabled(
        generation: u64,
        proxy_process_id: u64,
        ipv4_proxy_port_network_order: u16,
        ipv6_proxy_port_network_order: u16,
        ipv4_dns_port_network_order: u16,
        ipv6_dns_port_network_order: u16,
    ) -> Self {
        Self {
            magic: CONFIG_MAGIC,
            version: CONFIG_VERSION,
            size: CONFIG_SIZE,
            generation,
            proxy_process_id,
            ipv4_proxy_port_network_order,
            ipv6_proxy_port_network_order,
            ipv4_dns_port_network_order,
            ipv6_dns_port_network_order,
            flags: CONFIG_FLAG_DNS_REDIRECT | CONFIG_FLAG_TCP_REDIRECT | CONFIG_FLAG_UDP_DIVERT,
            reserved: 0,
        }
    }

    pub fn validate(&self) -> Result<(), WindowsWfpError> {
        if self.magic != CONFIG_MAGIC || self.version != CONFIG_VERSION || self.size != CONFIG_SIZE
        {
            return Err(WindowsWfpError::InvalidData("WFP 驱动配置版本无效"));
        }
        let known_flags =
            CONFIG_FLAG_DNS_REDIRECT | CONFIG_FLAG_TCP_REDIRECT | CONFIG_FLAG_UDP_DIVERT;
        if self.flags & !known_flags != 0 || self.reserved != 0 {
            return Err(WindowsWfpError::InvalidData("WFP 驱动配置包含未知标志"));
        }
        let process_valid =
            self.proxy_process_id != 0 && self.proxy_process_id <= u64::from(u32::MAX);
        if (self.flags == 0 && self.proxy_process_id != 0) || (self.flags != 0 && !process_valid) {
            return Err(WindowsWfpError::InvalidData("启用 WFP 重定向时进程无效"));
        }
        let dns_enabled = self.flags & CONFIG_FLAG_DNS_REDIRECT != 0;
        let tcp_enabled = self.flags & CONFIG_FLAG_TCP_REDIRECT != 0;
        if !port_pair_matches_flag(
            dns_enabled,
            self.ipv4_dns_port_network_order,
            self.ipv6_dns_port_network_order,
        ) || !port_pair_matches_flag(
            tcp_enabled,
            self.ipv4_proxy_port_network_order,
            self.ipv6_proxy_port_network_order,
        ) {
            return Err(WindowsWfpError::InvalidData(
                "WFP 重定向标志与监听端口不一致",
            ));
        }
        Ok(())
    }
}

const fn port_pair_matches_flag(enabled: bool, ipv4: u16, ipv6: u16) -> bool {
    if enabled {
        ipv4 != 0 && ipv6 != 0
    } else {
        ipv4 == 0 && ipv6 == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct WfpStatus {
    pub magic: u32,
    pub version: u16,
    pub size: u16,
    pub generation: u64,
    pub proxy_process_id: u64,
    pub flags: u32,
    pub active_classifications: u32,
    pub redirected_connections: u64,
    pub fail_open_connections: u64,
    pub queued_udp_datagrams: u64,
    pub dropped_udp_datagrams: u64,
    pub injected_udp_datagrams: u64,
}

impl WfpStatus {
    pub fn validate(&self) -> Result<(), WindowsWfpError> {
        if self.magic != STATUS_MAGIC || self.version != STATUS_VERSION || self.size != STATUS_SIZE
        {
            return Err(WindowsWfpError::InvalidData("WFP 驱动状态版本无效"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedirectContext {
    process_id: u64,
    original_local: [u8; 128],
    original_remote: [u8; 128],
    app_id: Vec<u8>,
}

impl RedirectContext {
    pub fn decode(bytes: &[u8]) -> Result<Self, WindowsWfpError> {
        if bytes.len() < usize::from(REDIRECT_CONTEXT_HEADER_SIZE) {
            return Err(WindowsWfpError::InvalidData("WFP redirect context 被截断"));
        }
        if read_u32(bytes, 0)? != REDIRECT_CONTEXT_MAGIC
            || read_u16(bytes, 4)? != REDIRECT_CONTEXT_VERSION
            || read_u16(bytes, 6)? != REDIRECT_CONTEXT_HEADER_SIZE
        {
            return Err(WindowsWfpError::InvalidData(
                "WFP redirect context 版本无效",
            ));
        }
        let total_size = usize::try_from(read_u32(bytes, 8)?)
            .map_err(|_| WindowsWfpError::InvalidData("WFP redirect context 长度溢出"))?;
        let app_id_length = usize::try_from(read_u32(bytes, 280)?)
            .map_err(|_| WindowsWfpError::InvalidData("WFP 应用身份长度溢出"))?;
        if total_size != bytes.len()
            || app_id_length > MAX_APP_ID_BYTES
            || total_size != usize::from(REDIRECT_CONTEXT_HEADER_SIZE) + app_id_length
        {
            return Err(WindowsWfpError::InvalidData(
                "WFP redirect context 长度不一致",
            ));
        }
        let mut original_local = [0_u8; 128];
        original_local.copy_from_slice(&bytes[24..152]);
        let mut original_remote = [0_u8; 128];
        original_remote.copy_from_slice(&bytes[152..280]);
        Ok(Self {
            process_id: read_u64(bytes, 16)?,
            original_local,
            original_remote,
            app_id: bytes[usize::from(REDIRECT_CONTEXT_HEADER_SIZE)..].to_vec(),
        })
    }

    #[must_use]
    pub const fn process_id(&self) -> u64 {
        self.process_id
    }

    #[must_use]
    pub const fn original_local(&self) -> &[u8; 128] {
        &self.original_local
    }

    #[must_use]
    pub const fn original_remote(&self) -> &[u8; 128] {
        &self.original_remote
    }

    #[must_use]
    pub fn app_id(&self) -> &[u8] {
        &self.app_id
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(WindowsWfpError::InvalidData("WFP 数据被截断"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(WindowsWfpError::InvalidData("WFP 数据被截断"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(WindowsWfpError::InvalidData("WFP 数据被截断"))?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_sizes_and_ioctl_codes_are_stable() {
        assert_eq!(size_of::<WfpConfig>(), usize::from(CONFIG_SIZE));
        assert_eq!(size_of::<WfpStatus>(), usize::from(STATUS_SIZE));
        assert_eq!(IOCTL_APPLY_CONFIG, 0x0012_A004);
        assert_eq!(IOCTL_QUERY_STATUS, 0x0012_6008);
        assert_eq!(IOCTL_RECEIVE_UDP, 0x0012_600c);
        assert_eq!(IOCTL_INJECT_UDP, 0x0012_a010);
    }

    #[test]
    fn enabled_config_requires_complete_redirect_target() {
        assert!(
            WfpConfig::enabled(4, 912, 10, 11, 12, 13)
                .validate()
                .is_ok()
        );
        assert!(WfpConfig::dns_only(4, 912, 12, 13).validate().is_ok());
        assert!(WfpConfig::enabled(4, 0, 10, 11, 12, 13).validate().is_err());
        let mut partial = WfpConfig::dns_only(4, 912, 12, 13);
        partial.ipv6_dns_port_network_order = 0;
        assert!(partial.validate().is_err());
        let mut dirty_disabled = WfpConfig::disabled(5);
        dirty_disabled.proxy_process_id = 912;
        assert!(dirty_disabled.validate().is_err());
        assert!(WfpConfig::disabled(5).validate().is_ok());
    }

    #[test]
    fn redirect_context_rejects_inconsistent_app_length() {
        let mut bytes = vec![0_u8; usize::from(REDIRECT_CONTEXT_HEADER_SIZE) + 2];
        bytes[0..4].copy_from_slice(&REDIRECT_CONTEXT_MAGIC.to_le_bytes());
        bytes[4..6].copy_from_slice(&REDIRECT_CONTEXT_VERSION.to_le_bytes());
        bytes[6..8].copy_from_slice(&REDIRECT_CONTEXT_HEADER_SIZE.to_le_bytes());
        let total_size = u32::try_from(bytes.len()).unwrap_or_default();
        bytes[8..12].copy_from_slice(&total_size.to_le_bytes());
        bytes[280..284].copy_from_slice(&3_u32.to_le_bytes());

        assert!(RedirectContext::decode(&bytes).is_err());
    }

    #[test]
    fn checked_in_driver_header_matches_rust_contract() {
        let header = include_str!("../../../platform/windows/include/nonproxy_wfp_abi.h");
        for required in [
            "#define NP_WFP_CONFIG_VERSION ((UINT16)3)",
            "C_ASSERT(sizeof(NP_WFP_CONFIG_V3) == 40);",
            "C_ASSERT(sizeof(NP_WFP_STATUS_V2) == 72);",
            "C_ASSERT(FIELD_OFFSET(NP_WFP_REDIRECT_CONTEXT_V1, AppId) == 288);",
            "C_ASSERT(FIELD_OFFSET(NP_WFP_UDP_DATAGRAM_V1, Data) == 88);",
            "C_ASSERT(FIELD_OFFSET(NP_WFP_UDP_INJECT_V1, Payload) == 80);",
            "CTL_CODE(FILE_DEVICE_NETWORK, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA)",
            "CTL_CODE(FILE_DEVICE_NETWORK, 0x802, METHOD_BUFFERED, FILE_READ_DATA)",
        ] {
            assert!(
                header.contains(required),
                "Windows Driver ABI header 缺少固定契约: {required}"
            );
        }
    }
}
