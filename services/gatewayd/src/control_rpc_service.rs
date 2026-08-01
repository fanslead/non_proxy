use std::{path::PathBuf, sync::Arc};

#[cfg(test)]
use crate::credential_store::OsCredentialStore;
use crate::{Gateway, credential_store::CredentialStore, session_capability::SessionCapability};
use nonproxy_exit_probe::ExitProbeClient;
use nonproxy_subscription::SubscriptionClient;

use crate::subscription_service::SubscriptionService;

const MAX_RPC_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct ControlRpcService {
    pub(crate) gateway: Gateway,
    pub(crate) session: SessionCapability,
    pub(crate) credential_store: Arc<dyn CredentialStore>,
    pub(crate) subscription_service: SubscriptionService,
    pub(crate) exit_probe_client: Option<ExitProbeClient>,
    pub(crate) diagnostics_directory: Option<PathBuf>,
}

impl ControlRpcService {
    #[cfg(test)]
    #[must_use]
    pub fn new(gateway: Gateway, session: SessionCapability) -> Self {
        let credential_store: Arc<dyn CredentialStore> = Arc::new(OsCredentialStore);
        Self {
            subscription_service: SubscriptionService::new(
                gateway.clone(),
                Arc::clone(&credential_store),
                Arc::new(SubscriptionClient::new()),
            ),
            gateway,
            session,
            credential_store,
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
            subscription_service: SubscriptionService::new(
                gateway.clone(),
                Arc::clone(&credential_store),
                Arc::new(SubscriptionClient::new()),
            ),
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
