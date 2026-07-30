use std::env;

use crate::GatewayError;

pub const CONTROL_PIPE_ENVIRONMENT: &str = "NONPROXY_WINDOWS_CONTROL_PIPE";
pub const FLOW_PIPE_ENVIRONMENT: &str = "NONPROXY_WINDOWS_FLOW_PIPE";
pub const PIPE_SDDL_ENVIRONMENT: &str = "NONPROXY_WINDOWS_PIPE_SDDL";
pub const DEFAULT_CONTROL_PIPE: &str = r"\\.\pipe\NonProxy.Control.v1";
pub const DEFAULT_FLOW_PIPE: &str = r"\\.\pipe\NonProxy.Flow.v1";
const PIPE_PREFIX: &str = r"\\.\pipe\NonProxy.";
const DEVELOPMENT_PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsTransportConfig {
    control_pipe: String,
    flow_pipe: String,
    pipe_sddl: String,
    production_security: bool,
}

impl WindowsTransportConfig {
    pub fn from_process() -> Result<Self, GatewayError> {
        let control_pipe = environment_text(CONTROL_PIPE_ENVIRONMENT, DEFAULT_CONTROL_PIPE)?;
        let flow_pipe = environment_text(FLOW_PIPE_ENVIRONMENT, DEFAULT_FLOW_PIPE)?;
        let configured_sddl = env::var(PIPE_SDDL_ENVIRONMENT);
        let (pipe_sddl, production_security) = match configured_sddl {
            Ok(value) => (value, true),
            Err(env::VarError::NotPresent) => (DEVELOPMENT_PIPE_SDDL.to_owned(), false),
            Err(env::VarError::NotUnicode(_)) => {
                return Err(GatewayError::InvalidLocalPath(
                    "Windows 命名管道安全描述符不是有效 UTF-8",
                ));
            }
        };
        Self::new(control_pipe, flow_pipe, pipe_sddl, production_security)
    }

    fn new(
        control_pipe: String,
        flow_pipe: String,
        pipe_sddl: String,
        production_security: bool,
    ) -> Result<Self, GatewayError> {
        validate_pipe_name(&control_pipe)?;
        validate_pipe_name(&flow_pipe)?;
        if control_pipe == flow_pipe {
            return Err(GatewayError::InvalidLocalPath(
                "Windows 控制和数据命名管道必须不同",
            ));
        }
        validate_sddl(&pipe_sddl)?;
        Ok(Self {
            control_pipe,
            flow_pipe,
            pipe_sddl,
            production_security,
        })
    }

    pub fn require_production_security(&self) -> Result<(), GatewayError> {
        if self.production_security {
            Ok(())
        } else {
            Err(GatewayError::InvalidLocalPath(
                "Windows Service 缺少安装器下发的命名管道 DACL",
            ))
        }
    }

    pub fn control_pipe(&self) -> &str {
        self.control_pipe.as_str()
    }

    pub fn flow_pipe(&self) -> &str {
        self.flow_pipe.as_str()
    }

    pub fn pipe_sddl(&self) -> &str {
        self.pipe_sddl.as_str()
    }
}

fn environment_text(name: &str, default: &str) -> Result<String, GatewayError> {
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(env::VarError::NotUnicode(_)) => Err(GatewayError::InvalidLocalPath(
            "Windows 命名管道配置不是有效 UTF-8",
        )),
    }
}

fn validate_pipe_name(value: &str) -> Result<(), GatewayError> {
    let suffix = value
        .strip_prefix(PIPE_PREFIX)
        .ok_or(GatewayError::InvalidLocalPath(
            "Windows 命名管道必须位于 NonProxy 本地命名空间",
        ))?;
    if suffix.is_empty()
        || value.len() > 160
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(GatewayError::InvalidLocalPath("Windows 命名管道名称无效"));
    }
    Ok(())
}

fn validate_sddl(value: &str) -> Result<(), GatewayError> {
    if value.len() > 1_024 || !value.starts_with("D:") || value.contains('\0') {
        return Err(GatewayError::InvalidLocalPath("Windows 命名管道 DACL 无效"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PIPE_PREFIX, WindowsTransportConfig};

    #[test]
    fn accepts_distinct_private_pipes_and_installer_sddl() {
        let config = WindowsTransportConfig::new(
            r"\\.\pipe\NonProxy.Custom.Control".to_owned(),
            r"\\.\pipe\NonProxy.Custom.Flow".to_owned(),
            "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)".to_owned(),
            true,
        );
        let Ok(config) = config else {
            panic!("合法 Windows 传输配置应通过校验: {config:?}");
        };

        assert_eq!(config.control_pipe(), r"\\.\pipe\NonProxy.Custom.Control");
        assert_eq!(config.flow_pipe(), r"\\.\pipe\NonProxy.Custom.Flow");
        assert_eq!(config.pipe_sddl(), "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)");
        assert!(config.require_production_security().is_ok());
    }

    #[test]
    fn development_sddl_cannot_start_windows_service() {
        let config = WindowsTransportConfig::new(
            r"\\.\pipe\NonProxy.Control.v1".to_owned(),
            r"\\.\pipe\NonProxy.Flow.v1".to_owned(),
            "D:P(A;;GA;;;SY)".to_owned(),
            false,
        );
        let Ok(config) = config else {
            panic!("开发配置本身应可用于控制台模式: {config:?}");
        };

        assert!(config.require_production_security().is_err());
    }

    #[test]
    fn accepts_exact_pipe_name_length_limit() {
        let control = format!("{PIPE_PREFIX}{}", "a".repeat(160 - PIPE_PREFIX.len()));
        let config = WindowsTransportConfig::new(
            control.clone(),
            r"\\.\pipe\NonProxy.Flow.v1".to_owned(),
            "D:P(A;;GA;;;SY)".to_owned(),
            true,
        );
        let Ok(config) = config else {
            panic!("160 字符的命名管道应通过边界校验: {config:?}");
        };

        assert_eq!(config.control_pipe(), control);
    }

    #[test]
    fn rejects_pipe_escape_collision_and_invalid_characters() {
        for (control, flow) in [
            (
                r"\\.\pipe\Other.Control".to_owned(),
                r"\\.\pipe\NonProxy.Flow.v1".to_owned(),
            ),
            (
                r"\\.\pipe\NonProxy.Same".to_owned(),
                r"\\.\pipe\NonProxy.Same".to_owned(),
            ),
            (
                r"\\.\pipe\NonProxy.Invalid\Child".to_owned(),
                r"\\.\pipe\NonProxy.Flow.v1".to_owned(),
            ),
            (
                format!("{PIPE_PREFIX}{}", "a".repeat(161 - PIPE_PREFIX.len())),
                r"\\.\pipe\NonProxy.Flow.v1".to_owned(),
            ),
        ] {
            assert!(
                WindowsTransportConfig::new(control, flow, "D:P(A;;GA;;;SY)".to_owned(), true,)
                    .is_err()
            );
        }
    }

    #[test]
    fn rejects_malformed_or_unbounded_sddl() {
        for sddl in [
            "O:SY".to_owned(),
            "D:\0P".to_owned(),
            format!("D:{}", "A".repeat(1_023)),
        ] {
            assert!(
                WindowsTransportConfig::new(
                    r"\\.\pipe\NonProxy.Control.v1".to_owned(),
                    r"\\.\pipe\NonProxy.Flow.v1".to_owned(),
                    sddl,
                    true,
                )
                .is_err()
            );
        }
    }
}
