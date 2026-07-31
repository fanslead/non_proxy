use std::{path::PathBuf, sync::Arc};

#[cfg(test)]
use crate::credential_store::OsCredentialStore;
use crate::{Gateway, credential_store::CredentialStore, session_capability::SessionCapability};
use nonproxy_exit_probe::ExitProbeClient;

const MAX_RPC_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct ControlRpcService {
    pub(crate) gateway: Gateway,
    pub(crate) session: SessionCapability,
    pub(crate) credential_store: Arc<dyn CredentialStore>,
    pub(crate) exit_probe_client: Option<ExitProbeClient>,
    pub(crate) diagnostics_directory: Option<PathBuf>,
}

impl ControlRpcService {
    #[cfg(test)]
    #[must_use]
    pub fn new(gateway: Gateway, session: SessionCapability) -> Self {
        Self {
            gateway,
            session,
            credential_store: Arc::new(OsCredentialStore),
            exit_probe_client: None,
            diagnostics_directory: None,
        }
    }

    pub(crate) fn with_credential_store(
        gateway: Gateway,
        session: SessionCapability,
        credential_store: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            gateway,
            session,
            credential_store,
            exit_probe_client: None,
            diagnostics_directory: None,
        }
    }

    #[must_use]
    pub(crate) fn with_exit_probe_client(mut self, client: Option<ExitProbeClient>) -> Self {
        self.exit_probe_client = client;
        self
    }

    #[must_use]
    pub(crate) fn with_diagnostics_directory(mut self, directory: PathBuf) -> Self {
        self.diagnostics_directory = Some(directory);
        self
    }

    #[must_use]
    pub const fn max_message_bytes() -> usize {
        MAX_RPC_MESSAGE_BYTES
    }
}
