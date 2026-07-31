use std::{
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use nonproxy_exit_probe::ExitProbeSigner;
use zeroize::Zeroizing;

use crate::{error::ProbeServerError, tls::read_secret_file};

const BIND_ENVIRONMENT: &str = "NONPROXY_PROBE_BIND";
const CERTIFICATE_ENVIRONMENT: &str = "NONPROXY_PROBE_TLS_CERT";
const TLS_KEY_ENVIRONMENT: &str = "NONPROXY_PROBE_TLS_KEY";
const SIGNING_KEY_ENVIRONMENT: &str = "NONPROXY_PROBE_SIGNING_KEY";
const MAXIMUM_CONNECTIONS_ENVIRONMENT: &str = "NONPROXY_PROBE_MAX_CONNECTIONS";
const DEFAULT_BIND: &str = "[::]:8443";
const DEFAULT_MAXIMUM_CONNECTIONS: usize = 1_024;
const MAXIMUM_CONNECTIONS_LIMIT: usize = 65_536;

pub struct ProbeServerConfig {
    bind_address: SocketAddr,
    certificate_path: PathBuf,
    tls_key_path: PathBuf,
    signing_key_path: PathBuf,
    maximum_connections: usize,
}

impl ProbeServerConfig {
    pub fn from_process() -> Result<Self, ProbeServerError> {
        let bind_address = env::var(BIND_ENVIRONMENT)
            .unwrap_or_else(|_| DEFAULT_BIND.to_owned())
            .parse::<SocketAddr>()
            .map_err(|_| ProbeServerError::Configuration)?;
        let certificate_path = required_absolute_path(CERTIFICATE_ENVIRONMENT)?;
        let tls_key_path = required_absolute_path(TLS_KEY_ENVIRONMENT)?;
        let signing_key_path = required_absolute_path(SIGNING_KEY_ENVIRONMENT)?;
        let maximum_connections = match env::var(MAXIMUM_CONNECTIONS_ENVIRONMENT) {
            Ok(value) => value
                .parse::<usize>()
                .ok()
                .filter(|value| (1..=MAXIMUM_CONNECTIONS_LIMIT).contains(value))
                .ok_or(ProbeServerError::Configuration)?,
            Err(env::VarError::NotPresent) => DEFAULT_MAXIMUM_CONNECTIONS,
            Err(env::VarError::NotUnicode(_)) => {
                return Err(ProbeServerError::Configuration);
            }
        };
        Ok(Self {
            bind_address,
            certificate_path,
            tls_key_path,
            signing_key_path,
            maximum_connections,
        })
    }

    #[must_use]
    pub const fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    #[must_use]
    pub fn certificate_path(&self) -> &Path {
        &self.certificate_path
    }

    #[must_use]
    pub fn tls_key_path(&self) -> &Path {
        &self.tls_key_path
    }

    #[must_use]
    pub const fn maximum_connections(&self) -> usize {
        self.maximum_connections
    }

    pub fn load_signer(&self) -> Result<ExitProbeSigner, ProbeServerError> {
        let bytes = Zeroizing::new(read_secret_file(&self.signing_key_path, Some(32))?);
        ExitProbeSigner::from_secret_bytes(&bytes).map_err(|_| ProbeServerError::SigningKey)
    }
}

fn required_absolute_path(name: &'static str) -> Result<PathBuf, ProbeServerError> {
    let value = env::var_os(name).ok_or(ProbeServerError::Configuration)?;
    let path = PathBuf::from(value);
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(ProbeServerError::Configuration);
    }
    Ok(path)
}
