use thiserror::Error;

const KEYRING_SERVICE: &str = "com.nonproxy.gatewayd.outbound";

pub trait CredentialStore: Send + Sync {
    fn set(&self, reference: &str, secret: &[u8]) -> Result<(), CredentialStoreError>;
    fn delete(&self, reference: &str) -> Result<(), CredentialStoreError>;
}

#[derive(Clone, Default)]
pub struct OsCredentialStore;

impl CredentialStore for OsCredentialStore {
    fn set(&self, reference: &str, secret: &[u8]) -> Result<(), CredentialStoreError> {
        entry(reference)?
            .set_secret(secret)
            .map_err(|_| CredentialStoreError::Operation("写入"))
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
    }

    impl CredentialStore for MemoryCredentialStore {
        fn set(&self, reference: &str, secret: &[u8]) -> Result<(), CredentialStoreError> {
            self.entries
                .lock()
                .map_err(|_| CredentialStoreError::Operation("锁定"))?
                .insert(reference.to_owned(), secret.to_vec());
            Ok(())
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
