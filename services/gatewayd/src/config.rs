use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::GatewayError;
#[cfg(windows)]
use crate::WindowsTransportConfig;
use nonproxy_exit_probe::{ExitProbeClient, ExitProbeEndpoint, ExitProbeVerifier};

const STATE_DIRECTORY_ENVIRONMENT: &str = "NONPROXY_STATE_DIR";
const SOCKET_PATH_ENVIRONMENT: &str = "NONPROXY_SOCKET_PATH";
const FLOW_SOCKET_PATH_ENVIRONMENT: &str = "NONPROXY_FLOW_SOCKET_PATH";
const BUNDLE_FINGERPRINT_ENVIRONMENT: &str = "NONPROXY_GATEWAY_BUNDLE_FINGERPRINT";
const MACOS_TEAM_IDENTIFIER_ENVIRONMENT: &str = "NONPROXY_MAC_TEAM_IDENTIFIER";
const EXIT_PROBE_ENDPOINT_ENVIRONMENT: &str = "NONPROXY_EXIT_PROBE_ENDPOINT";
const EXIT_PROBE_PUBLIC_KEY_ENVIRONMENT: &str = "NONPROXY_EXIT_PROBE_PUBLIC_KEY";
const DEVELOPMENT_FINGERPRINT: &str = "development";
#[cfg(target_os = "macos")]
const MACOS_APP_GROUP_STATE_PATH: &str =
    "Library/Group Containers/group.com.nonproxy.shared/Library/Application Support/NonProxy";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayConfig {
    state_directory: PathBuf,
    database_path: PathBuf,
    socket_path: PathBuf,
    flow_socket_path: PathBuf,
    bundle_fingerprint: String,
    macos_team_identifier: Option<String>,
    exit_probe: Option<ExitProbeConfig>,
    #[cfg(windows)]
    windows_transport: WindowsTransportConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitProbeConfig {
    endpoint: String,
    public_key: String,
}

impl ExitProbeConfig {
    fn from_environment() -> Result<Option<Self>, GatewayError> {
        Self::from_values(
            optional_environment(EXIT_PROBE_ENDPOINT_ENVIRONMENT)?,
            optional_environment(EXIT_PROBE_PUBLIC_KEY_ENVIRONMENT)?,
        )
    }

    fn from_values(
        endpoint: Option<String>,
        public_key: Option<String>,
    ) -> Result<Option<Self>, GatewayError> {
        match (endpoint, public_key) {
            (None, None) => Ok(None),
            (Some(endpoint), Some(public_key)) => {
                ExitProbeEndpoint::parse(&endpoint)
                    .and_then(|_| ExitProbeVerifier::from_public_key_base64(&public_key))
                    .map_err(|_| {
                        GatewayError::RuntimeIdentity("出口探针地址或 Ed25519 公钥无效".to_owned())
                    })?;
                Ok(Some(Self {
                    endpoint,
                    public_key,
                }))
            }
            _ => Err(GatewayError::RuntimeIdentity(
                "出口探针地址和公钥必须同时配置".to_owned(),
            )),
        }
    }

    pub(crate) fn client(&self) -> Result<ExitProbeClient, GatewayError> {
        let endpoint = ExitProbeEndpoint::parse(&self.endpoint)
            .map_err(|_| GatewayError::RuntimeIdentity("出口探针地址配置已损坏".to_owned()))?;
        let verifier = ExitProbeVerifier::from_public_key_base64(&self.public_key)
            .map_err(|_| GatewayError::RuntimeIdentity("出口探针公钥配置已损坏".to_owned()))?;
        ExitProbeClient::new(endpoint, verifier)
            .map_err(|_| GatewayError::RuntimeIdentity("出口探针 TLS 客户端初始化失败".to_owned()))
    }
}

impl GatewayConfig {
    pub fn from_process() -> Result<Self, GatewayError> {
        let state_directory = match env::var_os(STATE_DIRECTORY_ENVIRONMENT) {
            Some(value) => PathBuf::from(value),
            None => default_state_directory()?,
        };
        let socket_path = env::var_os(SOCKET_PATH_ENVIRONMENT)
            .map(PathBuf::from)
            .unwrap_or_else(|| state_directory.join("gatewayd.sock"));
        let flow_socket_path = env::var_os(FLOW_SOCKET_PATH_ENVIRONMENT)
            .map(PathBuf::from)
            .unwrap_or_else(|| state_directory.join("gatewayd-flow.sock"));
        let bundle_fingerprint = match env::var(BUNDLE_FINGERPRINT_ENVIRONMENT) {
            Ok(value) => validate_bundle_fingerprint(value)?,
            Err(env::VarError::NotPresent) => DEVELOPMENT_FINGERPRINT.to_owned(),
            Err(env::VarError::NotUnicode(_)) => {
                return Err(GatewayError::InvalidLocalPath(
                    "后台服务包指纹不是有效 UTF-8",
                ));
            }
        };
        let macos_team_identifier = optional_environment(MACOS_TEAM_IDENTIFIER_ENVIRONMENT)?
            .map(validate_team_identifier)
            .transpose()?;
        let exit_probe = ExitProbeConfig::from_environment()?;
        Self::build(
            state_directory,
            socket_path,
            flow_socket_path,
            bundle_fingerprint,
            macos_team_identifier,
            exit_probe,
        )
    }

    pub fn new(
        state_directory: impl Into<PathBuf>,
        socket_path: impl Into<PathBuf>,
    ) -> Result<Self, GatewayError> {
        let state_directory = state_directory.into();
        let socket_path = socket_path.into();
        let flow_socket_path = state_directory.join("gatewayd-flow.sock");
        Self::new_with_flow_socket(state_directory, socket_path, flow_socket_path)
    }

    pub fn new_with_flow_socket(
        state_directory: impl Into<PathBuf>,
        socket_path: impl Into<PathBuf>,
        flow_socket_path: impl Into<PathBuf>,
    ) -> Result<Self, GatewayError> {
        Self::build(
            state_directory.into(),
            socket_path.into(),
            flow_socket_path.into(),
            DEVELOPMENT_FINGERPRINT.to_owned(),
            None,
            None,
        )
    }

    fn build(
        state_directory: PathBuf,
        socket_path: PathBuf,
        flow_socket_path: PathBuf,
        bundle_fingerprint: String,
        macos_team_identifier: Option<String>,
        exit_probe: Option<ExitProbeConfig>,
    ) -> Result<Self, GatewayError> {
        validate_absolute(&state_directory)?;
        validate_absolute(&socket_path)?;
        validate_absolute(&flow_socket_path)?;
        if socket_path.parent() != Some(state_directory.as_path())
            || flow_socket_path.parent() != Some(state_directory.as_path())
            || socket_path == flow_socket_path
        {
            return Err(GatewayError::InvalidLocalPath(
                "控制和数据套接字必须是状态目录内的不同路径",
            ));
        }
        Ok(Self {
            database_path: state_directory.join("policy.sqlite3"),
            state_directory,
            socket_path,
            flow_socket_path,
            bundle_fingerprint,
            macos_team_identifier,
            exit_probe,
            #[cfg(windows)]
            windows_transport: WindowsTransportConfig::from_process()?,
        })
    }

    pub fn prepare(&self) -> Result<(), GatewayError> {
        reject_existing_state_path(&self.state_directory)?;
        fs::create_dir_all(&self.state_directory).map_err(|source| GatewayError::Io {
            operation: "创建状态目录",
            source,
        })?;
        validate_created_state_directory(&self.state_directory)?;
        restrict_directory(&self.state_directory)?;
        Ok(())
    }

    #[must_use]
    pub fn state_directory(&self) -> &Path {
        self.state_directory.as_path()
    }

    #[must_use]
    pub fn database_path(&self) -> &Path {
        self.database_path.as_path()
    }

    #[must_use]
    pub fn socket_path(&self) -> &Path {
        self.socket_path.as_path()
    }

    #[must_use]
    pub fn flow_socket_path(&self) -> &Path {
        self.flow_socket_path.as_path()
    }

    #[must_use]
    pub fn bundle_fingerprint(&self) -> &str {
        self.bundle_fingerprint.as_str()
    }

    #[must_use]
    pub fn macos_team_identifier(&self) -> Option<&str> {
        self.macos_team_identifier.as_deref()
    }

    #[must_use]
    pub(crate) const fn exit_probe(&self) -> Option<&ExitProbeConfig> {
        self.exit_probe.as_ref()
    }

    #[must_use]
    #[cfg(windows)]
    pub fn windows_transport(&self) -> &WindowsTransportConfig {
        &self.windows_transport
    }
}

fn optional_environment(name: &'static str) -> Result<Option<String>, GatewayError> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(GatewayError::RuntimeIdentity(format!(
            "{name} 不是有效 UTF-8"
        ))),
    }
}

fn validate_team_identifier(value: String) -> Result<String, GatewayError> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Ok(value);
    }
    Err(GatewayError::RuntimeIdentity(
        "macOS TeamIdentifier 格式无效".to_owned(),
    ))
}

fn validate_bundle_fingerprint(value: String) -> Result<String, GatewayError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(value);
    }
    Err(GatewayError::RuntimeIdentity(
        "后台服务包指纹必须是 64 位小写十六进制".to_owned(),
    ))
}

fn validate_absolute(path: &Path) -> Result<(), GatewayError> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(GatewayError::InvalidLocalPath("路径必须是绝对路径"));
    }
    Ok(())
}

fn reject_existing_state_path(path: &Path) -> Result<(), GatewayError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(GatewayError::InvalidLocalPath("状态目录不能是符号链接"))
        }
        Ok(metadata) if !metadata.is_dir() => {
            Err(GatewayError::InvalidLocalPath("状态目录路径已被文件占用"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GatewayError::Io {
            operation: "检查状态目录",
            source,
        }),
    }
}

fn validate_created_state_directory(path: &Path) -> Result<(), GatewayError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| GatewayError::Io {
        operation: "复核状态目录",
        source,
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GatewayError::InvalidLocalPath("状态目录类型无效"));
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), GatewayError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        GatewayError::Io {
            operation: "限制状态目录权限",
            source,
        }
    })
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), GatewayError> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn default_state_directory() -> Result<PathBuf, GatewayError> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(GatewayError::InvalidLocalPath("缺少用户主目录"))?;
    Ok(macos_state_directory(&home))
}

#[cfg(target_os = "macos")]
fn macos_state_directory(home: &Path) -> PathBuf {
    home.join(MACOS_APP_GROUP_STATE_PATH)
}

#[cfg(target_os = "windows")]
fn default_state_directory() -> Result<PathBuf, GatewayError> {
    let root = env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .ok_or(GatewayError::InvalidLocalPath("缺少 PROGRAMDATA"))?;
    Ok(root.join("NonProxy"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn default_state_directory() -> Result<PathBuf, GatewayError> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(GatewayError::InvalidLocalPath("缺少用户主目录"))?;
    Ok(home.join(".local/state/nonproxy"))
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use std::path::Path;
    use std::{fs, path::PathBuf};

    #[cfg(target_os = "macos")]
    use super::macos_state_directory;
    use super::{
        ExitProbeConfig, GatewayConfig, validate_bundle_fingerprint, validate_team_identifier,
    };

    #[test]
    fn rejects_socket_outside_state_directory() {
        let result = GatewayConfig::new(PathBuf::from("/tmp/nonproxy-a"), "/tmp/nonproxy.sock");

        assert!(result.is_err());
    }

    #[test]
    fn rejects_flow_socket_outside_or_equal_to_control_socket() {
        let state = PathBuf::from("/tmp/nonproxy-a");
        let control = state.join("gatewayd.sock");
        assert!(
            GatewayConfig::new_with_flow_socket(&state, &control, "/tmp/nonproxy-flow.sock")
                .is_err()
        );
        assert!(GatewayConfig::new_with_flow_socket(&state, &control, &control).is_err());
    }

    #[test]
    fn exit_probe_configuration_requires_a_valid_complete_trust_pair() {
        assert!(matches!(ExitProbeConfig::from_values(None, None), Ok(None)));
        assert!(
            ExitProbeConfig::from_values(Some("https://probe.example/v1/exit".to_owned()), None,)
                .is_err()
        );
        assert!(
            ExitProbeConfig::from_values(
                Some("http://probe.example/v1/exit".to_owned()),
                Some("invalid".to_owned()),
            )
            .is_err()
        );
        let signer = nonproxy_exit_probe::ExitProbeSigner::from_secret_bytes(&[7; 32])
            .unwrap_or_else(|error| panic!("测试出口探针密钥创建失败: {error}"));
        let configured = ExitProbeConfig::from_values(
            Some("https://probe.example/v1/exit".to_owned()),
            Some(signer.public_key_base64()),
        );
        assert!(matches!(configured, Ok(Some(_))));
    }

    #[test]
    fn rejects_state_path_occupied_by_file() {
        let directory = tempfile::tempdir();
        let Ok(directory) = directory else {
            panic!("临时目录创建失败: {directory:?}");
        };
        let state = directory.path().join("state");
        if let Err(error) = fs::write(&state, b"occupied") {
            panic!("状态占位文件创建失败: {error}");
        }
        let config = GatewayConfig::new(&state, state.join("gatewayd.sock"));
        let Ok(config) = config else {
            panic!("测试网关配置创建失败: {config:?}");
        };

        assert!(config.prepare().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_state_directory_symbolic_link_before_creation() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir();
        let target = tempfile::tempdir();
        let (Ok(directory), Ok(target)) = (directory, target) else {
            panic!("符号链接测试目录创建失败");
        };
        let state = directory.path().join("state");
        if let Err(error) = symlink(target.path(), &state) {
            panic!("状态目录符号链接创建失败: {error}");
        }
        let config = GatewayConfig::new(&state, state.join("gatewayd.sock"));
        let Ok(config) = config else {
            panic!("测试网关配置创建失败: {config:?}");
        };

        assert!(config.prepare().is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_default_uses_the_shared_app_group_container() {
        let path = macos_state_directory(Path::new("/Users/example"));

        assert_eq!(
            path,
            PathBuf::from(
                "/Users/example/Library/Group Containers/group.com.nonproxy.shared/\
                 Library/Application Support/NonProxy",
            )
        );
    }

    #[test]
    fn bundle_fingerprint_requires_canonical_sha256() {
        assert!(validate_bundle_fingerprint("a".repeat(64)).is_ok());
        assert!(validate_bundle_fingerprint("A".repeat(64)).is_err());
        assert!(validate_bundle_fingerprint("a".repeat(63)).is_err());
        assert!(validate_bundle_fingerprint("g".repeat(64)).is_err());
    }

    #[test]
    fn macos_team_identifier_rejects_empty_or_shell_like_values() {
        assert!(validate_team_identifier("TEAM123456".to_owned()).is_ok());
        assert!(validate_team_identifier(String::new()).is_err());
        assert!(validate_team_identifier("TEAM ID".to_owned()).is_err());
        assert!(validate_team_identifier("$(command)".to_owned()).is_err());
    }
}
