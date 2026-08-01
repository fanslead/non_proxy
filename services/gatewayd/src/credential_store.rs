use std::{collections::HashSet, sync::Arc};

use thiserror::Error;
use zeroize::Zeroizing;

const KEYRING_SERVICE: &str = "com.nonproxy.gatewayd.outbound";

pub trait CredentialStore: Send + Sync {
    fn set(&self, reference: &str, secret: &[u8]) -> Result<(), CredentialStoreError>;
    fn get(&self, reference: &str) -> Result<Vec<u8>, CredentialStoreError>;
    fn delete(&self, reference: &str) -> Result<(), CredentialStoreError>;
}

pub(crate) struct CredentialWrite {
    pub(crate) reference: String,
    pub(crate) secret: Zeroizing<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CredentialWriteFailure {
    cleanup_failures: usize,
}

impl CredentialWriteFailure {
    #[must_use]
    pub(crate) const fn cleanup_failures(self) -> usize {
        self.cleanup_failures
    }
}

pub(crate) async fn store_credentials(
    store: Arc<dyn CredentialStore>,
    credentials: Vec<CredentialWrite>,
) -> Result<HashSet<String>, CredentialWriteFailure> {
    let cleanup_store = Arc::clone(&store);
    let references = credentials
        .iter()
        .map(|credential| credential.reference.clone())
        .collect::<HashSet<_>>();
    let task = tokio::task::spawn_blocking(move || {
        let mut attempted = HashSet::new();
        for credential in credentials {
            attempted.insert(credential.reference.clone());
            if store
                .set(&credential.reference, credential.secret.as_slice())
                .is_err()
            {
                let cleanup_failures = delete_credentials_sync(store.as_ref(), attempted);
                return Err(CredentialWriteFailure { cleanup_failures });
            }
        }
        Ok(attempted)
    });
    match task.await {
        Ok(result) => result,
        Err(_) => Err(CredentialWriteFailure {
            cleanup_failures: delete_credentials(cleanup_store, references).await,
        }),
    }
}

pub(crate) async fn load_credential(
    store: Arc<dyn CredentialStore>,
    reference: String,
) -> Result<Zeroizing<Vec<u8>>, CredentialStoreError> {
    tokio::task::spawn_blocking(move || store.get(&reference).map(Zeroizing::new))
        .await
        .map_err(|_| CredentialStoreError::Operation("读取"))?
}

pub(crate) async fn delete_credentials(
    store: Arc<dyn CredentialStore>,
    references: HashSet<String>,
) -> usize {
    tokio::task::spawn_blocking(move || delete_credentials_sync(store.as_ref(), references))
        .await
        .map_or(1, std::convert::identity)
}

fn delete_credentials_sync(store: &dyn CredentialStore, references: HashSet<String>) -> usize {
    references
        .into_iter()
        .filter(|reference| store.delete(reference).is_err())
        .count()
}

#[derive(Clone, Default)]
pub struct OsCredentialStore;

impl CredentialStore for OsCredentialStore {
    fn set(&self, reference: &str, secret: &[u8]) -> Result<(), CredentialStoreError> {
        entry(reference)?
            .set_secret(secret)
            .map_err(|_| CredentialStoreError::Operation("写入"))
    }

    fn get(&self, reference: &str) -> Result<Vec<u8>, CredentialStoreError> {
        entry(reference)?
            .get_secret()
            .map_err(|_| CredentialStoreError::Operation("读取"))
    }

    fn delete(&self, reference: &str) -> Result<(), CredentialStoreError> {
        entry(reference)?
            .delete_credential()
            .map_err(|_| CredentialStoreError::Operation("删除"))
    }
}

fn entry(reference: &str) -> Result<keyring::Entry, CredentialStoreError> {
    keyring::Entry::new(KEYRING_SERVICE, reference)
        .map_err(|_| CredentialStoreError::Operation("打开"))
}

#[derive(Debug, Error)]
pub enum CredentialStoreError {
    #[error("系统凭据库{0}操作失败")]
    Operation(&'static str),
}

#[cfg(test)]
pub mod tests_support {
    use std::{collections::HashMap, sync::Mutex};

    use super::{CredentialStore, CredentialStoreError};

    #[derive(Default)]
    pub struct MemoryCredentialStore {
        entries: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl MemoryCredentialStore {
        pub fn contains(&self, reference: &str) -> bool {
            self.entries
                .lock()
                .map(|entries| entries.contains_key(reference))
                .unwrap_or(false)
        }

        pub fn is_empty(&self) -> bool {
            self.entries
                .lock()
                .map(|entries| entries.is_empty())
                .unwrap_or(false)
        }

        pub fn value(&self, reference: &str) -> Option<Vec<u8>> {
            self.entries
                .lock()
                .ok()
                .and_then(|entries| entries.get(reference).cloned())
        }
    }

    impl CredentialStore for MemoryCredentialStore {
        fn set(&self, reference: &str, secret: &[u8]) -> Result<(), CredentialStoreError> {
            self.entries
                .lock()
                .map_err(|_| CredentialStoreError::Operation("锁定"))?
                .insert(reference.to_owned(), secret.to_vec());
            Ok(())
        }

        fn get(&self, reference: &str) -> Result<Vec<u8>, CredentialStoreError> {
            self.entries
                .lock()
                .map_err(|_| CredentialStoreError::Operation("锁定"))?
                .get(reference)
                .cloned()
                .ok_or(CredentialStoreError::Operation("读取"))
        }

        fn delete(&self, reference: &str) -> Result<(), CredentialStoreError> {
            self.entries
                .lock()
                .map_err(|_| CredentialStoreError::Operation("锁定"))?
                .remove(reference);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
    };

    use zeroize::Zeroizing;

    use super::{CredentialStore, CredentialStoreError, CredentialWrite, store_credentials};

    #[tokio::test]
    async fn partial_write_removes_successful_and_failed_references() {
        let store = Arc::new(FailingCredentialStore::default());
        let credentials = vec![credential("first", b"one"), credential("failure", b"two")];

        let result = store_credentials(store.clone(), credentials).await;

        assert_eq!(
            result.map_err(super::CredentialWriteFailure::cleanup_failures),
            Err(0)
        );
        assert!(store.references().is_empty());
    }

    fn credential(reference: &str, secret: &[u8]) -> CredentialWrite {
        CredentialWrite {
            reference: reference.to_owned(),
            secret: Zeroizing::new(secret.to_vec()),
        }
    }

    #[derive(Default)]
    struct FailingCredentialStore {
        references: Mutex<HashSet<String>>,
    }

    impl FailingCredentialStore {
        fn references(&self) -> HashSet<String> {
            self.references
                .lock()
                .map(|values| values.clone())
                .unwrap_or_default()
        }
    }

    impl CredentialStore for FailingCredentialStore {
        fn set(&self, reference: &str, _secret: &[u8]) -> Result<(), CredentialStoreError> {
            self.references
                .lock()
                .map_err(|_| CredentialStoreError::Operation("测试锁定"))?
                .insert(reference.to_owned());
            if reference == "failure" {
                return Err(CredentialStoreError::Operation("测试写入"));
            }
            Ok(())
        }

        fn get(&self, _reference: &str) -> Result<Vec<u8>, CredentialStoreError> {
            Err(CredentialStoreError::Operation("测试读取"))
        }

        fn delete(&self, reference: &str) -> Result<(), CredentialStoreError> {
            self.references
                .lock()
                .map_err(|_| CredentialStoreError::Operation("测试锁定"))?
                .remove(reference);
            Ok(())
        }
    }
}
