use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AdapterHostConfig, AdapterHostError};

pub(crate) const RUNTIME_IDENTITY_FILE_NAME: &str = "adapter.runtime.json";
const RUNTIME_IDENTITY_SCHEMA_VERSION: u32 = 1;
const TEMPORARY_FILE_ATTEMPTS: usize = 4;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdapterRuntimeIdentity {
    schema_version: u32,
    bundle_fingerprint: String,
    process_id: u32,
    semantic_version: String,
    build_id: String,
}

#[derive(Debug)]
pub(crate) struct RuntimeIdentityGuard {
    path: PathBuf,
    content: Vec<u8>,
}

impl RuntimeIdentityGuard {
    pub(crate) fn create(config: &AdapterHostConfig) -> Result<Self, AdapterHostError> {
        let identity = AdapterRuntimeIdentity {
            schema_version: RUNTIME_IDENTITY_SCHEMA_VERSION,
            bundle_fingerprint: config.bundle_fingerprint().to_owned(),
            process_id: std::process::id(),
            semantic_version: env!("CARGO_PKG_VERSION").to_owned(),
            build_id: option_env!("NONPROXY_BUILD_ID")
                .unwrap_or("development")
                .to_owned(),
        };
        let content = serde_json::to_vec(&identity).map_err(|_| AdapterHostError::Configuration)?;
        let path = config.state_directory().join(RUNTIME_IDENTITY_FILE_NAME);
        reject_unsafe_target(&path)?;
        let (temporary_path, mut file) = create_temporary_file(config.state_directory())?;
        let write_result = file
            .write_all(&content)
            .and_then(|()| file.sync_all())
            .map_err(AdapterHostError::File);
        drop(file);
        if let Err(error) = write_result {
            let _cleanup_result = fs::remove_file(&temporary_path);
            return Err(error);
        }
        if let Err(error) = replace_file(&temporary_path, &path) {
            cleanup_failed_replacement(&temporary_path);
            return Err(AdapterHostError::File(error));
        }
        Ok(Self { path, content })
    }
}

impl Drop for RuntimeIdentityGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        if nonproxy_windows_security::validate_regular_file(&self.path).is_err() {
            return;
        }
        if matches!(fs::read(&self.path), Ok(content) if content == self.content) {
            let _cleanup_result = fs::remove_file(&self.path);
        }
    }
}

fn create_temporary_file(state_directory: &Path) -> Result<(PathBuf, File), AdapterHostError> {
    for _attempt in 0..TEMPORARY_FILE_ATTEMPTS {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(AdapterHostError::Random)?;
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = state_directory.join(format!(".{RUNTIME_IDENTITY_FILE_NAME}.{suffix}.tmp"));
        match open_private_file(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(AdapterHostError::File(error)),
        }
    }
    Err(AdapterHostError::Configuration)
}

fn open_private_file(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    if let Err(error) = nonproxy_windows_security::protect_current_user_file(path) {
        drop(file);
        let _cleanup = fs::remove_file(path);
        return Err(error);
    }
    Ok(file)
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    nonproxy_windows_security::replace_file_atomically(source, target)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn cleanup_failed_replacement(_path: &Path) {}

#[cfg(not(windows))]
fn cleanup_failed_replacement(path: &Path) {
    let _cleanup = fs::remove_file(path);
}

fn reject_unsafe_target(path: &Path) -> Result<(), AdapterHostError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(AdapterHostError::Configuration)
        }
        Ok(_) => {
            #[cfg(windows)]
            nonproxy_windows_security::validate_regular_file(path)
                .map_err(|_| AdapterHostError::Configuration)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AdapterHostError::File(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::{AdapterRuntimeIdentity, RUNTIME_IDENTITY_FILE_NAME, RuntimeIdentityGuard};
    use crate::AdapterHostConfig;

    #[test]
    fn creates_private_identity_and_removes_only_its_own_contents() {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let config =
            AdapterHostConfig::new(directory.path(), directory.path().join("adapter-host.sock"))
                .unwrap_or_else(|error| panic!("适配器配置创建失败: {error}"));
        let guard = RuntimeIdentityGuard::create(&config)
            .unwrap_or_else(|error| panic!("运行身份创建失败: {error}"));
        let path = directory.path().join(RUNTIME_IDENTITY_FILE_NAME);
        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("运行身份读取失败: {error}"));
        let identity = serde_json::from_slice::<AdapterRuntimeIdentity>(&bytes)
            .unwrap_or_else(|error| panic!("运行身份解码失败: {error}"));

        assert_eq!(identity.schema_version, 1);
        assert_eq!(identity.bundle_fingerprint, "development");
        assert_eq!(identity.process_id, std::process::id());
        assert_private_permissions(&path);

        drop(guard);
        assert!(!path.exists());

        let replacement_guard = RuntimeIdentityGuard::create(&config)
            .unwrap_or_else(|error| panic!("替换场景运行身份创建失败: {error}"));
        fs::write(&path, b"replacement")
            .unwrap_or_else(|error| panic!("替换运行身份失败: {error}"));
        drop(replacement_guard);
        assert_eq!(
            fs::read(&path).ok().as_deref(),
            Some(b"replacement".as_slice())
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_follow_runtime_identity_symbolic_link() {
        use std::os::unix::fs::symlink;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let outside = tempfile::NamedTempFile::new()
            .unwrap_or_else(|error| panic!("外部文件创建失败: {error}"));
        let path = directory.path().join(RUNTIME_IDENTITY_FILE_NAME);
        symlink(outside.path(), &path)
            .unwrap_or_else(|error| panic!("运行身份符号链接创建失败: {error}"));
        let config =
            AdapterHostConfig::new(directory.path(), directory.path().join("adapter-host.sock"))
                .unwrap_or_else(|error| panic!("适配器配置创建失败: {error}"));

        assert!(RuntimeIdentityGuard::create(&config).is_err());
    }

    #[cfg(unix)]
    fn assert_private_permissions(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(path)
            .unwrap_or_else(|error| panic!("运行身份元数据读取失败: {error}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_private_permissions(_path: &Path) {}
}
