use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::AdapterHostError;
#[cfg(windows)]
use crate::WindowsAdapterTransportConfig;

const STATE_DIRECTORY_ENVIRONMENT: &str = "NONPROXY_ADAPTER_STATE_DIR";
const SOCKET_PATH_ENVIRONMENT: &str = "NONPROXY_ADAPTER_SOCKET_PATH";
const BUNDLE_FINGERPRINT_ENVIRONMENT: &str = "NONPROXY_ADAPTER_BUNDLE_FINGERPRINT";
const CAPABILITY_FILE_NAME: &str = "adapter.capability";
const CATALOG_FILE_NAME: &str = "installations.json";
const DEVELOPMENT_FINGERPRINT: &str = "development";
#[cfg(target_os = "macos")]
const MACOS_STATE_PATH: &str = "Library/Group Containers/group.com.nonproxy.shared/Library/Application Support/NonProxy/adapter-host";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterHostConfig {
    state_directory: PathBuf,
    socket_path: PathBuf,
    bundle_fingerprint: String,
    #[cfg(windows)]
    windows_transport: WindowsAdapterTransportConfig,
}

impl AdapterHostConfig {
    pub fn from_process() -> Result<Self, AdapterHostError> {
        let state_directory = env::var_os(STATE_DIRECTORY_ENVIRONMENT)
            .map(PathBuf::from)
            .map_or_else(default_state_directory, Ok)?;
        let socket_path = env::var_os(SOCKET_PATH_ENVIRONMENT)
            .map(PathBuf::from)
            .unwrap_or_else(|| state_directory.join("adapter-host.sock"));
        let bundle_fingerprint = match env::var(BUNDLE_FINGERPRINT_ENVIRONMENT) {
            Ok(value) => validate_bundle_fingerprint(value)?,
            Err(env::VarError::NotPresent) => DEVELOPMENT_FINGERPRINT.to_owned(),
            Err(env::VarError::NotUnicode(_)) => return Err(AdapterHostError::Configuration),
        };
        Self::build(state_directory, socket_path, bundle_fingerprint)
    }

    pub fn new(
        state_directory: impl Into<PathBuf>,
        socket_path: impl Into<PathBuf>,
    ) -> Result<Self, AdapterHostError> {
        Self::build(
            state_directory.into(),
            socket_path.into(),
            DEVELOPMENT_FINGERPRINT.to_owned(),
        )
    }

    fn build(
        state_directory: PathBuf,
        socket_path: PathBuf,
        bundle_fingerprint: String,
    ) -> Result<Self, AdapterHostError> {
        if !state_directory.is_absolute()
            || !socket_path.is_absolute()
            || socket_path.parent() != Some(state_directory.as_path())
        {
            return Err(AdapterHostError::Configuration);
        }
        Ok(Self {
            state_directory,
            socket_path,
            bundle_fingerprint,
            #[cfg(windows)]
            windows_transport: WindowsAdapterTransportConfig::from_process()?,
        })
    }

    pub fn prepare(&self) -> Result<(), AdapterHostError> {
        reject_symlink(&self.state_directory)?;
        fs::create_dir_all(&self.state_directory).map_err(AdapterHostError::File)?;
        reject_symlink(&self.state_directory)?;
        let metadata = fs::metadata(&self.state_directory).map_err(AdapterHostError::File)?;
        if !metadata.is_dir() {
            return Err(AdapterHostError::Configuration);
        }
        #[cfg(unix)]
        fs::set_permissions(&self.state_directory, fs::Permissions::from_mode(0o700))
            .map_err(AdapterHostError::File)?;
        Ok(())
    }

    #[must_use]
    pub fn state_directory(&self) -> &Path {
        &self.state_directory
    }

    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    #[must_use]
    pub fn bundle_fingerprint(&self) -> &str {
        &self.bundle_fingerprint
    }

    #[must_use]
    pub fn catalog_path(&self) -> PathBuf {
        self.state_directory.join(CATALOG_FILE_NAME)
    }

    #[must_use]
    pub fn transaction_directory(&self) -> PathBuf {
        self.state_directory.join("transactions")
    }

    #[must_use]
    pub const fn capability_file_name(&self) -> &'static str {
        CAPABILITY_FILE_NAME
    }

    #[cfg(windows)]
    #[must_use]
    pub fn windows_transport(&self) -> &WindowsAdapterTransportConfig {
        &self.windows_transport
    }
}

fn validate_bundle_fingerprint(value: String) -> Result<String, AdapterHostError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Ok(value);
    }
    Err(AdapterHostError::Configuration)
}

fn reject_symlink(path: &Path) -> Result<(), AdapterHostError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AdapterHostError::Configuration),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AdapterHostError::File(error)),
    }
}

#[cfg(target_os = "macos")]
fn default_state_directory() -> Result<PathBuf, AdapterHostError> {
    let home = env::var_os("HOME").ok_or(AdapterHostError::Configuration)?;
    Ok(PathBuf::from(home).join(MACOS_STATE_PATH))
}

#[cfg(target_os = "windows")]
fn default_state_directory() -> Result<PathBuf, AdapterHostError> {
    let local_app_data = env::var_os("LOCALAPPDATA").ok_or(AdapterHostError::Configuration)?;
    Ok(PathBuf::from(local_app_data).join("NonProxy/adapter-host"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn default_state_directory() -> Result<PathBuf, AdapterHostError> {
    let home = env::var_os("HOME").ok_or(AdapterHostError::Configuration)?;
    Ok(PathBuf::from(home).join(".nonproxy/adapter-host"))
}

#[cfg(test)]
mod tests {
    use super::{AdapterHostConfig, validate_bundle_fingerprint};

    #[test]
    fn socket_must_be_a_direct_child_of_state_directory() {
        let valid =
            AdapterHostConfig::new("/tmp/nonproxy-adapter", "/tmp/nonproxy-adapter/host.sock");
        let escaped = AdapterHostConfig::new("/tmp/nonproxy-adapter", "/tmp/host.sock");

        assert!(valid.is_ok());
        assert!(escaped.is_err());
    }

    #[test]
    fn bundle_fingerprint_requires_canonical_sha256() {
        assert!(validate_bundle_fingerprint("a".repeat(64)).is_ok());
        assert!(validate_bundle_fingerprint("A".repeat(64)).is_err());
        assert!(validate_bundle_fingerprint("a".repeat(63)).is_err());
        assert!(validate_bundle_fingerprint("g".repeat(64)).is_err());
    }
}
