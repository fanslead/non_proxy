#[cfg(not(target_os = "windows"))]
const REQUIRED_PROVIDER_IDS: &[&str] = &["transparent-proxy", "dns-proxy"];

#[cfg(target_os = "windows")]
const REQUIRED_PROVIDER_IDS: &[&str] = &["windows-wfp", "windows-dns"];

#[must_use]
pub const fn required_provider_ids() -> &'static [&'static str] {
    REQUIRED_PROVIDER_IDS
}
