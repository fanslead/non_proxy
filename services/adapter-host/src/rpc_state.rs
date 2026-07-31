use std::{path::PathBuf, sync::Arc};

use nonproxy_adapter_transaction::AdapterTransactionManager;
use nonproxy_local_auth::{SessionCapability, validate_operation_id};
use nonproxy_proto::adapter::v1::AdapterRequestContext;
use tonic::Status;

use crate::{AdapterHostError, catalog::InstallationCatalog};

const MAXIMUM_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct AdapterRpcService {
    pub(crate) catalog: Arc<InstallationCatalog>,
    pub(crate) transactions: Arc<AdapterTransactionManager>,
    pub(crate) mutation_gate: Arc<tokio::sync::Mutex<()>>,
    session: SessionCapability,
}

impl AdapterRpcService {
    pub fn open(
        catalog_path: impl Into<PathBuf>,
        transaction_directory: impl Into<PathBuf>,
        session: SessionCapability,
    ) -> Result<Self, AdapterHostError> {
        Ok(Self::new(
            Arc::new(InstallationCatalog::open(catalog_path)?),
            Arc::new(AdapterTransactionManager::open(transaction_directory)?),
            session,
        ))
    }

    #[must_use]
    pub(crate) fn new(
        catalog: Arc<InstallationCatalog>,
        transactions: Arc<AdapterTransactionManager>,
        session: SessionCapability,
    ) -> Self {
        Self {
            catalog,
            transactions,
            mutation_gate: Arc::new(tokio::sync::Mutex::new(())),
            session,
        }
    }

    #[must_use]
    pub const fn max_message_bytes() -> usize {
        MAXIMUM_MESSAGE_BYTES
    }

    pub(crate) fn authenticate<'a>(
        &self,
        context: Option<&'a AdapterRequestContext>,
        legacy_operation_id: Option<&str>,
    ) -> Result<&'a str, Status> {
        let context = context.ok_or_else(|| Status::unauthenticated("缺少适配器请求上下文"))?;
        validate_operation_id(&context.operation_id)
            .map_err(|_| Status::invalid_argument("operation_id 无效"))?;
        if legacy_operation_id
            .is_some_and(|legacy| !legacy.is_empty() && legacy != context.operation_id)
        {
            return Err(Status::invalid_argument("operation_id 不一致"));
        }
        if !self.session.matches(&context.session_capability_token) {
            return Err(Status::permission_denied("适配器会话能力令牌无效"));
        }
        Ok(&context.operation_id)
    }
}
