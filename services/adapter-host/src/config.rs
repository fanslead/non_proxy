use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::AdapterHostError;

const STATE_DIRECTORY_ENVIRONMENT: &str = "NONPROXY_ADAPTER_STATE_DIR";
const SOCKET_PATH_ENVIRONMENT: &str = "NONPROXY_ADAPTER_SOCKET_PATH";
const CAPABILITY_FILE_NAME: &str = "adapter.capability";
const CATALOG_FILE_NAME: &str = "installations.json";
#[cfg(target_os = "macos")]
const MACOS_STATE_PATH: &str = "Library/Group Containers/group.com.nonproxy.shared/Library/Application Support/NonProxy/adapter-host";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterHostConfig {
    state_directory: PathBuf,
    socket_path: PathBuf,
}

impl AdapterHostConfig {
    pub fn from_process() -> Result<Self, AdapterHostError> {
        let state_directory = env::var_os(STATE_DIRECTORY_ENVIRONMENT)
            .map(PathBuf::from)
            .map_or_else(default_state_directory, Ok)?;
        let socket_path = env::var_os(SOCKET_PATH_ENVIRONMENT)
            .map(PathBuf::from)
            .unwrap_or_else(|| state_directory.join("adapter-host.sock"));
        Self::new(state_directory, socket_path)
    }

    pub fn new(
        state_directory: impl Into<PathBuf>,
        socket_path: impl Into<PathBuf>,
    ) -> Result<Self, AdapterHostError> {
        let state_directory = state_directory.into();
        let socket_path = socket_path.into();
        if !state_directory.is_absolute()
            || !socket_path.is_absolute()
            || socket_path.parent() != Some(state_directory.as_path())
        {
            return Err(AdapterHostError::Configuration);
        }
        Ok(Self {
            state_directory,
            socket_path,
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

#[cfg(not(target_os = "macos"))]
fn default_state_directory() -> Result<PathBuf, AdapterHostError> {
    let home = env::var_os("HOME").ok_or(AdapterHostError::Configuration)?;
    Ok(PathBuf::from(home).join(".nonproxy/adapter-host"))
}

#[cfg(test)]
mod tests {
    use super::AdapterHostConfig;

    #[test]
    fn socket_must_be_a_direct_child_of_state_directory() {
        let valid =
            AdapterHostConfig::new("/tmp/nonproxy-adapter", "/tmp/nonproxy-adapter/host.sock");
        let escaped = AdapterHostConfig::new("/tmp/nonproxy-adapter", "/tmp/host.sock");

        assert!(valid.is_ok());
        assert!(escaped.is_err());
    }
}
