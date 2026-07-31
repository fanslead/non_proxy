mod abi;
mod error;
mod udp;

#[cfg(windows)]
mod bfe;
#[cfg(windows)]
mod driver;
#[cfg(windows)]
mod redirect;

pub use abi::{
    CONFIG_FLAG_DNS_REDIRECT, CONFIG_FLAG_TCP_REDIRECT, CONFIG_FLAG_UDP_DIVERT, CONFIG_SIZE,
    CONFIG_VERSION, IOCTL_APPLY_CONFIG, IOCTL_INJECT_UDP, IOCTL_QUERY_STATUS, IOCTL_RECEIVE_UDP,
    MAX_APP_ID_BYTES, REDIRECT_CONTEXT_HEADER_SIZE, REDIRECT_CONTEXT_VERSION, RedirectContext,
    WfpConfig, WfpStatus,
};
pub use error::WindowsWfpError;
pub use udp::{
    MAX_UDP_BATCH_BYTES, MAX_UDP_PAYLOAD_BYTES, UdpDatagram, UdpInjection, UdpInjectionContext,
    decode_udp_batch,
};

#[cfg(windows)]
pub use bfe::DynamicWfpSession;
#[cfg(windows)]
pub use driver::WfpDriver;
#[cfg(windows)]
pub use redirect::{RedirectMetadata, apply_redirect_records, query_redirect_metadata};
