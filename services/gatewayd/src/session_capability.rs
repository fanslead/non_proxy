use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use nonproxy_proto::control::v1::OperationContext;
use tonic::Status;

use crate::GatewayError;

pub const CONTROL_CAPABILITY_FILE_NAME: &str = "session.capability";
pub const PROVIDER_CAPABILITY_FILE_NAME: &str = "provider.capability";
const SESSION_TOKEN_LENGTH: usize = 32;
const MAX_OPERATION_ID_LENGTH: usize = 128;
const TEMPORARY_FILE_ATTEMPTS: usize = 4;

#[derive(Clone)]
pub struct SessionCapability {
    token: [u8; SESSION_TOKEN_LENGTH],
}

impl SessionCapability {
    pub fn create_control(state_directory: &Path) -> Result<Self, GatewayError> {
        Self::create_named(state_directory, CONTROL_CAPABILITY_FILE_NAME)
    }

    pub fn create_provider(state_directory: &Path) -> Result<Self, GatewayError> {
        Self::create_named(state_directory, PROVIDER_CAPABILITY_FILE_NAME)
    }

    fn create_named(state_directory: &Path, file_name: &str) -> Result<Self, GatewayError> {
        let mut token = [0_u8; SESSION_TOKEN_LENGTH];
        getrandom::fill(&mut token).map_err(|error| GatewayError::Random(error.to_string()))?;
        let capability = Self { token };
        capability.write_to(state_directory, file_name)?;
        Ok(capability)
    }

    #[cfg(test)]
    #[must_use]
    pub const fn from_token(token: [u8; SESSION_TOKEN_LENGTH]) -> Self {
        Self { token }
    }

    pub fn validate(&self, context: Option<&OperationContext>) -> Result<(), Status> {
        let context = context.ok_or_else(|| Status::unauthenticated("缺少操作上下文"))?;
        validate_operation_id(&context.operation_id)?;
        self.validate_token(&context.session_capability_token)
    }

    pub fn validate_token(&self, token: &[u8]) -> Result<(), Status> {
        if constant_time_equal(&self.token, token) {
            Ok(())
        } else {
            Err(Status::permission_denied("会话能力令牌无效"))
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn token(&self) -> &[u8] {
        &self.token
    }

    fn write_to(&self, state_directory: &Path, file_name: &str) -> Result<(), GatewayError> {
        let path = state_directory.join(file_name);
        reject_symlink(&path)?;
        let (temporary_path, mut file) = create_temporary_file(state_directory, file_name)?;
        let write_result = file
            .write_all(&self.token)
            .and_then(|()| file.sync_all())
            .map_err(|source| GatewayError::Io {
                operation: "写入会话能力令牌",
                source,
            });
        drop(file);
        if let Err(error) = write_result {
            let _cleanup_result = fs::remove_file(&temporary_path);
            return Err(error);
        }
        let replace_result = replace_capability_file(&temporary_path, &path);
        if replace_result.is_err() {
            let _cleanup_result = fs::remove_file(&temporary_path);
        }
        replace_result
    }
}

fn create_temporary_file(
    state_directory: &Path,
    file_name: &str,
) -> Result<(PathBuf, File), GatewayError> {
    for _attempt in 0..TEMPORARY_FILE_ATTEMPTS {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(|error| GatewayError::Random(error.to_string()))?;
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = state_directory.join(format!(".{file_name}.{suffix}.tmp"));
        match open_private_file(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(source) => {
                return Err(GatewayError::Io {
                    operation: "创建临时会话能力令牌",
                    source,
                });
            }
        }
    }
    Err(GatewayError::InvalidLocalPath(
        "无法分配临时会话能力令牌路径",
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

#[cfg(unix)]
fn replace_capability_file(source: &Path, target: &Path) -> Result<(), GatewayError> {
    fs::rename(source, target).map_err(|source| GatewayError::Io {
        operation: "原子替换会话能力令牌",
        source,
    })
}

#[cfg(not(unix))]
fn replace_capability_file(source: &Path, target: &Path) -> Result<(), GatewayError> {
    if target.exists() {
        fs::remove_file(target).map_err(|source| GatewayError::Io {
            operation: "移除旧会话能力令牌",
            source,
        })?;
    }
    fs::rename(source, target).map_err(|source| GatewayError::Io {
        operation: "替换会话能力令牌",
        source,
    })
}

fn validate_operation_id(value: &str) -> Result<(), Status> {
    if value.is_empty()
        || value.len() > MAX_OPERATION_ID_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(Status::invalid_argument("operation_id 无效"));
    }
    Ok(())
}

fn constant_time_equal(expected: &[u8; SESSION_TOKEN_LENGTH], actual: &[u8]) -> bool {
    if actual.len() != SESSION_TOKEN_LENGTH {
        return false;
    }
    let difference = expected
        .iter()
        .zip(actual)
        .fold(0_u8, |value, (left, right)| value | (left ^ right));
    difference == 0
}

fn reject_symlink(path: &Path) -> Result<(), GatewayError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(GatewayError::InvalidLocalPath("会话能力令牌不能是符号链接"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GatewayError::Io {
            operation: "检查会话能力令牌路径",
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{
        CONTROL_CAPABILITY_FILE_NAME, PROVIDER_CAPABILITY_FILE_NAME, SESSION_TOKEN_LENGTH,
        SessionCapability,
    };

    #[test]
    fn creates_private_token_file_with_matching_bytes() {
        let directory = tempfile::tempdir();
        let Ok(directory) = directory else {
            panic!("临时目录创建失败: {directory:?}");
        };

        let capability = match SessionCapability::create_control(directory.path()) {
            Ok(capability) => capability,
            Err(error) => panic!("会话能力令牌创建失败: {error}"),
        };
        let bytes = fs::read(directory.path().join(CONTROL_CAPABILITY_FILE_NAME));
        let Ok(bytes) = bytes else {
            panic!("会话能力令牌读取失败: {bytes:?}");
        };

        assert_eq!(bytes.len(), SESSION_TOKEN_LENGTH);
        assert_eq!(bytes, capability.token());
        assert_private_permissions(directory.path().join(CONTROL_CAPABILITY_FILE_NAME));
    }

    #[test]
    fn creates_distinct_provider_capability() {
        let directory = tempfile::tempdir();
        let Ok(directory) = directory else {
            panic!("临时目录创建失败: {directory:?}");
        };

        let control = SessionCapability::create_control(directory.path());
        let provider = SessionCapability::create_provider(directory.path());
        let (Ok(control), Ok(provider)) = (control, provider) else {
            panic!("控制面或 Provider 能力令牌创建失败");
        };

        assert_ne!(control.token(), provider.token());
        assert_private_permissions(directory.path().join(PROVIDER_CAPABILITY_FILE_NAME));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_follow_existing_symbolic_link() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir();
        let target = tempfile::NamedTempFile::new();
        let (Ok(directory), Ok(target)) = (directory, target) else {
            panic!("符号链接测试夹具创建失败");
        };
        let link = directory.path().join(CONTROL_CAPABILITY_FILE_NAME);
        if let Err(error) = symlink(target.path(), &link) {
            panic!("测试符号链接创建失败: {error}");
        }

        let result = SessionCapability::create_control(directory.path());

        assert!(result.is_err());
        let target_bytes = fs::read(target.path());
        assert!(matches!(target_bytes, Ok(bytes) if bytes.is_empty()));
    }

    #[cfg(unix)]
    fn assert_private_permissions(path: PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path);
        let Ok(metadata) = metadata else {
            panic!("会话能力令牌元数据读取失败: {metadata:?}");
        };
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_private_permissions(_path: PathBuf) {}
}
