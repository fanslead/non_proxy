use std::path::Path;

#[cfg(test)]
use nonproxy_local_auth::SESSION_TOKEN_LENGTH;
use nonproxy_local_auth::{SessionCapability as LocalSessionCapability, validate_operation_id};
use nonproxy_proto::control::v1::OperationContext;
use tonic::Status;

use crate::GatewayError;

pub const CONTROL_CAPABILITY_FILE_NAME: &str = "session.capability";
pub const PROVIDER_CAPABILITY_FILE_NAME: &str = "provider.capability";

#[derive(Clone)]
pub struct SessionCapability {
    inner: LocalSessionCapability,
}

impl SessionCapability {
    pub fn create_control(state_directory: &Path) -> Result<Self, GatewayError> {
        Self::create_named(state_directory, CONTROL_CAPABILITY_FILE_NAME)
    }

    pub fn create_provider(state_directory: &Path) -> Result<Self, GatewayError> {
        Self::create_named(state_directory, PROVIDER_CAPABILITY_FILE_NAME)
    }

    fn create_named(state_directory: &Path, file_name: &str) -> Result<Self, GatewayError> {
        Ok(Self {
            inner: LocalSessionCapability::create(state_directory, file_name)?,
        })
    }

    #[cfg(test)]
    #[must_use]
    pub const fn from_token(token: [u8; SESSION_TOKEN_LENGTH]) -> Self {
        Self {
            inner: LocalSessionCapability::from_token(token),
        }
    }

    pub fn validate(&self, context: Option<&OperationContext>) -> Result<(), Status> {
        let context = context.ok_or_else(|| Status::unauthenticated("缺少操作上下文"))?;
        validate_operation_id(&context.operation_id)
            .map_err(|_| Status::invalid_argument("operation_id 无效"))?;
        self.validate_token(&context.session_capability_token)
    }

    pub fn validate_token(&self, token: &[u8]) -> Result<(), Status> {
        if self.matches_token(token) {
            Ok(())
        } else {
            Err(Status::permission_denied("会话能力令牌无效"))
        }
    }

    #[must_use]
    pub(crate) fn matches_token(&self, token: &[u8]) -> bool {
        self.inner.matches(token)
    }

    #[cfg(test)]
    #[must_use]
    pub fn token(&self) -> &[u8] {
        self.inner.token()
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
        restrict_test_directory(directory.path());

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
        restrict_test_directory(directory.path());

        let control = SessionCapability::create_control(directory.path());
        let provider = SessionCapability::create_provider(directory.path());
        let (Ok(control), Ok(provider)) = (control, provider) else {
            panic!("控制面或 Provider 能力令牌创建失败");
        };

        assert_ne!(control.token(), provider.token());
        assert_private_permissions(directory.path().join(PROVIDER_CAPABILITY_FILE_NAME));
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

    #[cfg(unix)]
    fn restrict_test_directory(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("测试状态目录权限设置失败: {error}"));
    }

    #[cfg(not(unix))]
    fn restrict_test_directory(_path: &std::path::Path) {}
}
