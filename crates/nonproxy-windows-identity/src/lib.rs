//! Windows WFP application identity decoding and trusted process signer resolution.
//!
//! Pure App ID and certificate fingerprint logic remains testable on every host. Win32 process,
//! WFP and WinTrust calls are isolated in `native` and only compiled for Windows.

mod app_id;
mod package_id;
#[cfg(any(windows, test))]
mod resolver;

#[cfg(windows)]
mod native;

pub use app_id::{certificate_signer_identity, decode_wfp_app_id};
pub use package_id::{
    decode_package_sid, package_publisher_signer_identity, package_stable_identity,
};

#[cfg(windows)]
pub use resolver::WindowsAppIdentityResolver;
