use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{GatewayConfig, GatewayError};

pub const RUNTIME_IDENTITY_FILE_NAME: &str = "gateway.runtime.json";
const RUNTIME_IDENTITY_SCHEMA_VERSION: u32 = 1;
const TEMPORARY_FILE_ATTEMPTS: usize = 4;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRuntimeIdentity {
    pub schema_version: u32,
    pub bundle_fingerprint: String,
    pub process_id: u32,
    pub semantic_version: String,
    pub build_id: String,
}

#[derive(Debug)]
pub struct RuntimeIdentityGuard {
    path: PathBuf,
    content: Vec<u8>,
}

impl RuntimeIdentityGuard {
    pub fn create(config: &GatewayConfig) -> Result<Self, GatewayError> {
        let identity = GatewayRuntimeIdentity {
            schema_version: RUNTIME_IDENTITY_SCHEMA_VERSION,
            bundle_fingerprint: config.bundle_fingerprint().to_owned(),
            process_id: std::process::id(),
            semantic_version: env!("CARGO_PKG_VERSION").to_owned(),
            build_id: option_env!("NONPROXY_BUILD_ID")
                .unwrap_or("development")
                .to_owned(),
        };
        let content = serde_json::to_vec(&identity)
            .map_err(|error| GatewayError::RuntimeIdentity(error.to_string()))?;
        let path = config.state_directory().join(RUNTIME_IDENTITY_FILE_NAME);
        reject_unsafe_target(&path)?;
        let (temporary_path, mut file) = create_temporary_file(config.state_directory())?;
        let write_result = file
            .write_all(&content)
            .and_then(|()| file.sync_all())
            .map_err(|source| GatewayError::Io {
                operation: "写入后台运行身份",
                source,
            });
        drop(file);
        if let Err(error) = write_result {
            let _cleanup_result = fs::remove_file(&temporary_path);
            return Err(error);
        }
        if let Err(error) = replace_identity_file(&temporary_path, &path) {
            let _cleanup_result = fs::remove_file(&temporary_path);
            return Err(error);
        }
        Ok(Self { path, content })
    }
}

impl Drop for RuntimeIdentityGuard {
    fn drop(&mut self) {
        if matches!(fs::read(&self.path), Ok(content) if content == self.content) {
            let _cleanup_result = fs::remove_file(&self.path);
        }
    }
}

fn create_temporary_file(state_directory: &Path) -> Result<(PathBuf, File), GatewayError> {
    for _attempt in 0..TEMPORARY_FILE_ATTEMPTS {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(|error| GatewayError::Random(error.to_string()))?;
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = state_directory.join(format!(".{RUNTIME_IDENTITY_FILE_NAME}.{suffix}.tmp"));
        match open_private_file(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(GatewayError::Io {
                    operation: "创建临时后台运行身份",
                    source,
                });
            }
        }
    }
    Err(GatewayError::InvalidLocalPath(
        "无法分配临时后台运行身份路径",
    ))
}

fn open_private_file(path: &Path) -> Result<File, std::io::Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600);
    }
    options.open(path)
}

fn reject_unsafe_target(path: &Path) -> Result<(), GatewayError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(GatewayError::InvalidLocalPath("后台运行身份不能是符号链接"))
        }
        Ok(metadata) if !metadata.is_file() => {
            Err(GatewayError::InvalidLocalPath("后台运行身份路径类型无效"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GatewayError::Io {
            operation: "检查后台运行身份路径",
            source,
        }),
    }
}

#[cfg(unix)]
fn replace_identity_file(source: &Path, target: &Path) -> Result<(), GatewayError> {
    fs::rename(source, target).map_err(|source| GatewayError::Io {
        operation: "原子替换后台运行身份",
        source,
    })
}

#[cfg(not(unix))]
fn replace_identity_file(source: &Path, target: &Path) -> Result<(), GatewayError> {
    if target.exists() {
        fs::remove_file(target).map_err(|source| GatewayError::Io {
            operation: "移除旧后台运行身份",
            source,
        })?;
    }
    fs::rename(source, target).map_err(|source| GatewayError::Io {
        operation: "替换后台运行身份",
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{GatewayRuntimeIdentity, RUNTIME_IDENTITY_FILE_NAME, RuntimeIdentityGuard};
    use crate::GatewayConfig;

    #[test]
    fn creates_private_identity_and_removes_its_own_file() {
        let directory = tempfile::tempdir();
        let Ok(directory) = directory else {
            panic!("临时目录创建失败: {directory:?}");
        };
        let config = GatewayConfig::new(directory.path(), directory.path().join("gatewayd.sock"));
        let Ok(config) = config else {
            panic!("测试网关配置创建失败: {config:?}");
        };
        let guard = RuntimeIdentityGuard::create(&config);
        let Ok(guard) = guard else {
            panic!("后台运行身份创建失败: {guard:?}");
        };
        let path = directory.path().join(RUNTIME_IDENTITY_FILE_NAME);
        let bytes = fs::read(&path);
        let Ok(bytes) = bytes else {
            panic!("后台运行身份读取失败: {bytes:?}");
        };
        let identity = serde_json::from_slice::<GatewayRuntimeIdentity>(&bytes);
        let Ok(identity) = identity else {
            panic!("后台运行身份解码失败: {identity:?}");
        };

        assert_eq!(identity.schema_version, 1);
        assert_eq!(identity.bundle_fingerprint, "development");
        assert_eq!(identity.process_id, std::process::id());
        assert_private_permissions(&path);

        drop(guard);
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_follow_identity_symbolic_link() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir();
        let target = tempfile::NamedTempFile::new();
        let (Ok(directory), Ok(target)) = (directory, target) else {
            panic!("符号链接测试夹具创建失败");
        };
        let path = directory.path().join(RUNTIME_IDENTITY_FILE_NAME);
        if let Err(error) = symlink(target.path(), &path) {
            panic!("后台运行身份符号链接创建失败: {error}");
        }
        let config = GatewayConfig::new(directory.path(), directory.path().join("gatewayd.sock"));
        let Ok(config) = config else {
            panic!("测试网关配置创建失败: {config:?}");
        };

        assert!(RuntimeIdentityGuard::create(&config).is_err());
    }

    #[cfg(unix)]
    fn assert_private_permissions(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path);
        let Ok(metadata) = metadata else {
            panic!("后台运行身份元数据读取失败: {metadata:?}");
        };
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_private_permissions(_path: &std::path::Path) {}

    #[cfg(unix)]
    use std::path::Path;
}
