use std::sync::{Arc, Mutex};

use nonproxy_storage::PolicyDatabase;

use crate::GatewayError;

#[derive(Clone)]
pub struct DatabaseExecutor {
    inner: Arc<Mutex<PolicyDatabase>>,
}

impl DatabaseExecutor {
    #[must_use]
    pub fn new(database: PolicyDatabase) -> Self {
        Self {
            inner: Arc::new(Mutex::new(database)),
        }
    }

    pub async fn run<TResult, TOperation>(
        &self,
        operation: TOperation,
    ) -> Result<TResult, GatewayError>
    where
        TResult: Send + 'static,
        TOperation: FnOnce(&mut PolicyDatabase) -> Result<TResult, GatewayError> + Send + 'static,
    {
        let database = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = database
                .lock()
                .map_err(|_| GatewayError::StateLockPoisoned("数据库"))?;
            operation(&mut guard)
        })
        .await
        .map_err(|error| GatewayError::DatabaseTask(error.to_string()))?
    }
}
