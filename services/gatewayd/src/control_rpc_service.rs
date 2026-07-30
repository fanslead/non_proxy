use std::sync::Arc;

#[cfg(test)]
use crate::credential_store::OsCredentialStore;
use crate::{Gateway, credential_store::CredentialStore, session_capability::SessionCapability};

const MAX_RPC_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct ControlRpcService {
    pub(crate) gateway: Gateway,
    pub(crate) session: SessionCapability,
    pub(crate) credential_store: Arc<dyn CredentialStore>,
}

impl ControlRpcService {
    #[cfg(test)]
    #[must_use]
    pub fn new(gateway: Gateway, session: SessionCapability) -> Self {
        Self {
            gateway,
            session,
            credential_store: Arc::new(OsCredentialStore),
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
        }
    }

    #[must_use]
    pub const fn max_message_bytes() -> usize {
        MAX_RPC_MESSAGE_BYTES
    }
}
