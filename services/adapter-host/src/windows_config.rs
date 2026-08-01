use std::env;

use nonproxy_windows_ipc::{validate_nonproxy_pipe_name, validate_pipe_sddl};

use crate::AdapterHostError;

pub const ADAPTER_PIPE_ENVIRONMENT: &str = "NONPROXY_WINDOWS_ADAPTER_PIPE";
pub const PIPE_SDDL_ENVIRONMENT: &str = "NONPROXY_WINDOWS_ADAPTER_PIPE_SDDL";
pub const DEFAULT_ADAPTER_PIPE: &str = r"\\.\pipe\NonProxy.Adapter.v1";
const DEVELOPMENT_PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsAdapterTransportConfig {
    pipe: String,
    pipe_sddl: String,
    production_security: bool,
}

impl WindowsAdapterTransportConfig {
    pub fn from_process() -> Result<Self, AdapterHostError> {
        let pipe = environment_text(ADAPTER_PIPE_ENVIRONMENT, DEFAULT_ADAPTER_PIPE)?;
        let configured_sddl = env::var(PIPE_SDDL_ENVIRONMENT);
        let (pipe_sddl, production_security) = match configured_sddl {
            Ok(value) => (value, true),
            Err(env::VarError::NotPresent) => (DEVELOPMENT_PIPE_SDDL.to_owned(), false),
            Err(env::VarError::NotUnicode(_)) => return Err(AdapterHostError::Configuration),
        };
        Self::new(pipe, pipe_sddl, production_security)
    }

    fn new(
        pipe: String,
        pipe_sddl: String,
        production_security: bool,
    ) -> Result<Self, AdapterHostError> {
        validate_nonproxy_pipe_name(&pipe).map_err(|_| AdapterHostError::Configuration)?;
        validate_pipe_sddl(&pipe_sddl).map_err(|_| AdapterHostError::Configuration)?;
        Ok(Self {
            pipe,
            pipe_sddl,
            production_security,
        })
    }

    #[must_use]
    pub fn pipe(&self) -> &str {
        self.pipe.as_str()
    }

    #[must_use]
    pub fn pipe_sddl(&self) -> &str {
        self.pipe_sddl.as_str()
    }

    pub fn require_production_security(&self) -> Result<(), AdapterHostError> {
        self.production_security
            .then_some(())
            .ok_or(AdapterHostError::Configuration)
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

    #[test]
    fn accepts_private_product_pipe_and_installer_sddl() {
        let config = WindowsAdapterTransportConfig::new(
            r"\\.\pipe\NonProxy.Custom.Adapter".to_owned(),
            "D:P(A;;GA;;;SY)(A;;GRGW;;;IU)".to_owned(),
            true,
        );
        let Ok(config) = config else {
            panic!("合法 Windows Adapter 传输配置应通过校验: {config:?}");
        };

        assert_eq!(config.pipe(), r"\\.\pipe\NonProxy.Custom.Adapter");
        assert!(config.require_production_security().is_ok());
    }

    #[test]
    fn rejects_wrong_namespace_and_unconfigured_production_security() {
        assert!(
            WindowsAdapterTransportConfig::new(
                r"\\.\pipe\Other.Adapter".to_owned(),
                "D:P(A;;GA;;;SY)".to_owned(),
                true,
            )
            .is_err()
        );
        let development = WindowsAdapterTransportConfig::new(
            r"\\.\pipe\NonProxy.Adapter.v1".to_owned(),
            "D:P(A;;GA;;;SY)".to_owned(),
            false,
        );
        assert!(matches!(development, Ok(value) if value.require_production_security().is_err()));
    }
}
