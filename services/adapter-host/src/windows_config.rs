use std::env;

use nonproxy_windows_ipc::{
    adapter_pipe_name_for_user_sid, adapter_pipe_sddl_for_user_sid, current_process_user_sid,
    validate_nonproxy_pipe_name, validate_pipe_sddl,
};

use crate::AdapterHostError;

pub const ADAPTER_PIPE_ENVIRONMENT: &str = "NONPROXY_WINDOWS_ADAPTER_PIPE";
pub const PIPE_SDDL_ENVIRONMENT: &str = "NONPROXY_WINDOWS_ADAPTER_PIPE_SDDL";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsAdapterTransportConfig {
    pipe: String,
    pipe_sddl: String,
}

impl WindowsAdapterTransportConfig {
    pub fn from_process() -> Result<Self, AdapterHostError> {
        let user_sid = current_process_user_sid().map_err(|_| AdapterHostError::Configuration)?;
        let default_pipe = adapter_pipe_name_for_user_sid(&user_sid)
            .map_err(|_| AdapterHostError::Configuration)?;
        let required_sddl = adapter_pipe_sddl_for_user_sid(&user_sid)
            .map_err(|_| AdapterHostError::Configuration)?;
        let pipe = environment_text(ADAPTER_PIPE_ENVIRONMENT, &default_pipe)?;
        let pipe_sddl = environment_text(PIPE_SDDL_ENVIRONMENT, &required_sddl)?;
        Self::new(pipe, pipe_sddl, &required_sddl)
    }

    fn new(pipe: String, pipe_sddl: String, required_sddl: &str) -> Result<Self, AdapterHostError> {
        validate_nonproxy_pipe_name(&pipe).map_err(|_| AdapterHostError::Configuration)?;
        validate_pipe_sddl(&pipe_sddl).map_err(|_| AdapterHostError::Configuration)?;
        if pipe_sddl != required_sddl {
            return Err(AdapterHostError::Configuration);
        }
        Ok(Self { pipe, pipe_sddl })
    }

    #[must_use]
    pub fn pipe(&self) -> &str {
        self.pipe.as_str()
    }

    #[must_use]
    pub fn pipe_sddl(&self) -> &str {
        self.pipe_sddl.as_str()
    }
}

fn environment_text(name: &str, default: &str) -> Result<String, AdapterHostError> {
    match env::var(name) {
        Ok(value) => Ok(value),
        Err(env::VarError::NotPresent) => Ok(default.to_owned()),
        Err(env::VarError::NotUnicode(_)) => Err(AdapterHostError::Configuration),
    }
}

#[cfg(test)]
mod tests {
    use super::WindowsAdapterTransportConfig;

    const USER_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;S-1-5-21-1000-2000-3000-1001)";

    #[test]
    fn accepts_private_product_pipe_and_installer_sddl() {
        let config = WindowsAdapterTransportConfig::new(
            r"\\.\pipe\NonProxy.Custom.Adapter".to_owned(),
            USER_SDDL.to_owned(),
            USER_SDDL,
        );
        let Ok(config) = config else {
            panic!("合法 Windows Adapter 传输配置应通过校验: {config:?}");
        };

        assert_eq!(config.pipe(), r"\\.\pipe\NonProxy.Custom.Adapter");
        assert_eq!(config.pipe_sddl(), USER_SDDL);
    }

    #[test]
    fn rejects_wrong_namespace_and_broader_sddl() {
        assert!(
            WindowsAdapterTransportConfig::new(
                r"\\.\pipe\Other.Adapter".to_owned(),
                USER_SDDL.to_owned(),
                USER_SDDL,
            )
            .is_err()
        );
        assert!(
            WindowsAdapterTransportConfig::new(
                r"\\.\pipe\NonProxy.Adapter.v1".to_owned(),
                "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)".to_owned(),
                USER_SDDL,
            )
            .is_err()
        );
    }
}
