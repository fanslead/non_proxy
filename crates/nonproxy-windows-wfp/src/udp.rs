use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use crate::{MAX_APP_ID_BYTES, MAX_PACKAGE_SID_BYTES, WindowsWfpError};

pub const MAX_UDP_BATCH_BYTES: usize = 256 * 1024;
pub const MAX_UDP_PAYLOAD_BYTES: usize = 65_000;

const BATCH_MAGIC: u32 = u32::from_le_bytes(*b"NPUB");
const DATAGRAM_MAGIC: u32 = u32::from_le_bytes(*b"NPUD");
const INJECTION_MAGIC: u32 = u32::from_le_bytes(*b"NPUI");
const UDP_ABI_VERSION: u16 = 2;
const BATCH_HEADER_SIZE: usize = 16;
const DATAGRAM_HEADER_SIZE: usize = 96;
const INJECTION_HEADER_SIZE: usize = 80;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 23;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UdpDatagram {
    packet_id: u64,
    process_id: u64,
    compartment_id: u32,
    interface_index: u32,
    sub_interface_index: u32,
    local: SocketAddr,
    remote: SocketAddr,
    app_id: Vec<u8>,
    package_sid: Vec<u8>,
    payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UdpInjectionContext {
    packet_id: u64,
    compartment_id: u32,
    interface_index: u32,
    sub_interface_index: u32,
    local: SocketAddr,
    remote: SocketAddr,
}

impl UdpDatagram {
    #[must_use]
    pub const fn packet_id(&self) -> u64 {
        self.packet_id
    }

    #[must_use]
    pub const fn process_id(&self) -> u64 {
        self.process_id
    }

    #[must_use]
    pub const fn compartment_id(&self) -> u32 {
        self.compartment_id
    }

    #[must_use]
    pub const fn interface_index(&self) -> u32 {
        self.interface_index
    }

    #[must_use]
    pub const fn sub_interface_index(&self) -> u32 {
        self.sub_interface_index
    }

    #[must_use]
    pub const fn local(&self) -> SocketAddr {
        self.local
    }

    #[must_use]
    pub const fn remote(&self) -> SocketAddr {
        self.remote
    }

    #[must_use]
    pub fn app_id(&self) -> &[u8] {
        &self.app_id
    }

    #[must_use]
    pub fn package_sid(&self) -> &[u8] {
        &self.package_sid
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    #[must_use]
    pub const fn injection_context(&self) -> UdpInjectionContext {
        UdpInjectionContext {
            packet_id: self.packet_id,
            compartment_id: self.compartment_id,
            interface_index: self.interface_index,
            sub_interface_index: self.sub_interface_index,
            local: self.local,
            remote: self.remote,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UdpInjection {
    bytes: Vec<u8>,
}

impl UdpInjection {
    pub fn encode(context: UdpInjectionContext, payload: &[u8]) -> Result<Self, WindowsWfpError> {
        validate_payload(payload)?;
        let total_size = INJECTION_HEADER_SIZE
            .checked_add(payload.len())
            .ok_or(WindowsWfpError::InvalidData("WFP UDP 注入长度溢出"))?;
        let total_u32 = u32::try_from(total_size)
            .map_err(|_| WindowsWfpError::InvalidData("WFP UDP 注入长度溢出"))?;
        let payload_u32 = u32::try_from(payload.len())
            .map_err(|_| WindowsWfpError::InvalidData("WFP UDP payload 长度溢出"))?;
        let (family, local_address) = encode_address(context.local)?;
        let (remote_family, remote_address) = encode_address(context.remote)?;
        if family != remote_family {
            return Err(WindowsWfpError::InvalidData("WFP UDP 地址族不一致"));
        }
        let mut bytes = vec![0_u8; total_size];
        write_u32(&mut bytes, 0, INJECTION_MAGIC);
        write_u16(&mut bytes, 4, UDP_ABI_VERSION);
        write_u16(&mut bytes, 6, INJECTION_HEADER_SIZE as u16);
        write_u32(&mut bytes, 8, total_u32);
        write_u16(&mut bytes, 12, family);
        write_u64(&mut bytes, 16, context.packet_id);
        write_u32(&mut bytes, 24, context.compartment_id);
        write_u32(&mut bytes, 28, context.interface_index);
        write_u32(&mut bytes, 32, context.sub_interface_index);
        bytes[36..38].copy_from_slice(&context.local.port().to_be_bytes());
        bytes[38..40].copy_from_slice(&context.remote.port().to_be_bytes());
        bytes[40..56].copy_from_slice(&local_address);
        bytes[56..72].copy_from_slice(&remote_address);
        write_u32(&mut bytes, 72, payload_u32);
        bytes[INJECTION_HEADER_SIZE..].copy_from_slice(payload);
        Ok(Self { bytes })
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub fn decode_udp_batch(bytes: &[u8]) -> Result<Vec<UdpDatagram>, WindowsWfpError> {
    if bytes.len() < BATCH_HEADER_SIZE
        || read_u32(bytes, 0)? != BATCH_MAGIC
        || read_u16(bytes, 4)? != UDP_ABI_VERSION
        || usize::from(read_u16(bytes, 6)?) != BATCH_HEADER_SIZE
    {
        return Err(WindowsWfpError::InvalidData("WFP UDP batch 版本无效"));
    }
    let total = usize_from_u32(read_u32(bytes, 8)?)?;
    let count = usize_from_u32(read_u32(bytes, 12)?)?;
    if total != bytes.len() || total > MAX_UDP_BATCH_BYTES {
        return Err(WindowsWfpError::InvalidData("WFP UDP batch 长度无效"));
    }
    let mut cursor = BATCH_HEADER_SIZE;
    let mut datagrams = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let remaining = bytes
            .get(cursor..)
            .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据报被截断"))?;
        let record_size = usize_from_u32(read_u32(remaining, 8)?)?;
        let record = remaining
            .get(..record_size)
            .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据报被截断"))?;
        datagrams.push(decode_datagram(record)?);
        cursor = cursor
            .checked_add(record_size)
            .ok_or(WindowsWfpError::InvalidData("WFP UDP batch 长度溢出"))?;
    }
    if cursor != bytes.len() {
        return Err(WindowsWfpError::InvalidData("WFP UDP batch 存在尾随数据"));
    }
    Ok(datagrams)
}

fn decode_datagram(bytes: &[u8]) -> Result<UdpDatagram, WindowsWfpError> {
    if bytes.len() < DATAGRAM_HEADER_SIZE
        || read_u32(bytes, 0)? != DATAGRAM_MAGIC
        || read_u16(bytes, 4)? != UDP_ABI_VERSION
        || usize::from(read_u16(bytes, 6)?) != DATAGRAM_HEADER_SIZE
        || read_u16(bytes, 14)? != 0
        || read_u32(bytes, 92)? != 0
    {
        return Err(WindowsWfpError::InvalidData("WFP UDP 数据报版本无效"));
    }
    let total = usize_from_u32(read_u32(bytes, 8)?)?;
    let app_length = usize_from_u32(read_u32(bytes, 80)?)?;
    let package_sid_length = usize_from_u32(read_u32(bytes, 84)?)?;
    let payload_length = usize_from_u32(read_u32(bytes, 88)?)?;
    let expected = DATAGRAM_HEADER_SIZE
        .checked_add(app_length)
        .and_then(|value| value.checked_add(package_sid_length))
        .and_then(|value| value.checked_add(payload_length))
        .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据报长度溢出"))?;
    if total != bytes.len()
        || total != expected
        || app_length > MAX_APP_ID_BYTES
        || package_sid_length > MAX_PACKAGE_SID_BYTES
        || payload_length > MAX_UDP_PAYLOAD_BYTES
    {
        return Err(WindowsWfpError::InvalidData("WFP UDP 数据报长度无效"));
    }
    let family = read_u16(bytes, 12)?;
    let local_port = read_port(bytes, 44)?;
    let remote_port = read_port(bytes, 46)?;
    let interface_index = read_u32(bytes, 36)?;
    let local = decode_address(family, &bytes[48..64], local_port, interface_index)?;
    let remote = decode_address(family, &bytes[64..80], remote_port, interface_index)?;
    let package_sid_start = DATAGRAM_HEADER_SIZE + app_length;
    let payload_start = package_sid_start + package_sid_length;
    Ok(UdpDatagram {
        packet_id: read_u64(bytes, 16)?,
        process_id: read_u64(bytes, 24)?,
        compartment_id: read_u32(bytes, 32)?,
        interface_index,
        sub_interface_index: read_u32(bytes, 40)?,
        local,
        remote,
        app_id: bytes[DATAGRAM_HEADER_SIZE..package_sid_start].to_vec(),
        package_sid: bytes[package_sid_start..payload_start].to_vec(),
        payload: bytes[payload_start..].to_vec(),
    })
}

fn decode_address(
    family: u16,
    bytes: &[u8],
    port: u16,
    scope_id: u32,
) -> Result<SocketAddr, WindowsWfpError> {
    match family {
        AF_INET if bytes[4..].iter().all(|value| *value == 0) => Ok(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3])),
            port,
        )),
        AF_INET6 => {
            let octets: [u8; 16] = bytes
                .try_into()
                .map_err(|_| WindowsWfpError::InvalidData("WFP UDP IPv6 地址无效"))?;
            Ok(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(octets),
                port,
                0,
                scope_id,
            )))
        }
        _ => Err(WindowsWfpError::InvalidData("WFP UDP 地址族无效")),
    }
}

fn encode_address(address: SocketAddr) -> Result<(u16, [u8; 16]), WindowsWfpError> {
    if address.port() == 0 {
        return Err(WindowsWfpError::InvalidData("WFP UDP 端口无效"));
    }
    let mut bytes = [0_u8; 16];
    let family = match address.ip() {
        IpAddr::V4(value) => {
            bytes[..4].copy_from_slice(&value.octets());
            AF_INET
        }
        IpAddr::V6(value) => {
            bytes.copy_from_slice(&value.octets());
            AF_INET6
        }
    };
    Ok((family, bytes))
}

fn validate_payload(payload: &[u8]) -> Result<(), WindowsWfpError> {
    if payload.len() > MAX_UDP_PAYLOAD_BYTES {
        Err(WindowsWfpError::InvalidData("WFP UDP payload 长度无效"))
    } else {
        Ok(())
    }
}

fn read_port(bytes: &[u8], offset: usize) -> Result<u16, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据被截断"))?;
    let port = u16::from_be_bytes([value[0], value[1]]);
    (port != 0)
        .then_some(port)
        .ok_or(WindowsWfpError::InvalidData("WFP UDP 端口无效"))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据被截断"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据被截断"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, WindowsWfpError> {
    let value = bytes
        .get(offset..offset + 8)
        .ok_or(WindowsWfpError::InvalidData("WFP UDP 数据被截断"))?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

fn usize_from_u32(value: u32) -> Result<usize, WindowsWfpError> {
    usize::try_from(value).map_err(|_| WindowsWfpError::InvalidData("WFP UDP 长度溢出"))
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
#[path = "udp_tests.rs"]
mod tests;
