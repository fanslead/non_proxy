use std::{collections::HashSet, sync::Arc};

use nonproxy_proto::{
    common::v1::ErrorDetail,
    control::v1::{ImportConfigurationRequest, ImportConfigurationResponse},
};
use zeroize::Zeroizing;

use crate::{
    Gateway, GatewayError,
    credential_store::CredentialStore,
    outbound_import::{OutboundImportError, PreparedCredential, prepare},
};

pub async fn import(
    gateway: &Gateway,
    credential_store: Arc<dyn CredentialStore>,
    request: ImportConfigurationRequest,
) -> ImportConfigurationResponse {
    let configuration = Zeroizing::new(request.configuration);
    let current = match gateway.list_outbounds().await {
        Ok(value) => value,
        Err(error) => return gateway_failure(error),
    };
    let import_id = match new_import_id() {
        Ok(value) => value,
        Err(error) => return gateway_failure(error),
    };
    let prepared = match prepare(
        &request.format,
        configuration.as_slice(),
        import_id,
        &current,
    ) {
        Ok(value) => value,
        Err(error) => return import_failure(&error),
    };
    let summaries = prepared
        .outbounds
        .iter()
        .map(|(value, _)| crate::control_mapping::outbound_summary(value))
        .collect();
    if request.validate_only {
        return ImportConfigurationResponse {
            import_id: prepared.import_id,
            outbounds: summaries,
            warnings: prepared.warnings,
            error: None,
        };
    }

    let new_references =
        match store_new_credentials(Arc::clone(&credential_store), prepared.credentials).await {
            Ok(value) => value,
            Err(cleanup_failures) => {
                return credential_failure(
                    "代理凭据写入失败，出口配置没有改变。",
                    cleanup_failures,
                );
            }
        };
    if let Err(error) = gateway.save_outbounds(prepared.outbounds).await {
        let cleanup_failures =
            delete_credentials(Arc::clone(&credential_store), new_references).await;
        let mut response = gateway_failure(error);
        append_cleanup_warning(&mut response, cleanup_failures);
        return response;
    }

    let mut warnings = prepared.warnings;
    let stale = prepared
        .replaced_credential_references
        .into_iter()
        .filter(|reference| !new_references.contains(reference))
        .collect();
    let cleanup_failures = delete_credentials(credential_store, stale).await;
    append_warning(&mut warnings, cleanup_failures);
    ImportConfigurationResponse {
        import_id: prepared.import_id,
        outbounds: summaries,
        warnings,
        error: None,
    }
}

async fn store_new_credentials(
    store: Arc<dyn CredentialStore>,
    credentials: Vec<PreparedCredential>,
) -> Result<HashSet<String>, usize> {
    let task = tokio::task::spawn_blocking(move || {
        let mut stored: HashSet<String> = HashSet::new();
        for credential in credentials {
            if store
                .set(&credential.reference, credential.secret.as_slice())
                .is_err()
            {
                let cleanup_failures = stored
                    .iter()
                    .filter(|reference| store.delete(reference).is_err())
                    .count();
                return Err(cleanup_failures);
            }
            stored.insert(credential.reference);
        }
        Ok(stored)
    })
    .await;
    task.unwrap_or(Err(1))
}

async fn delete_credentials(store: Arc<dyn CredentialStore>, references: HashSet<String>) -> usize {
    let task = tokio::task::spawn_blocking(move || {
        references
            .into_iter()
            .filter(|reference| store.delete(reference).is_err())
            .count()
    })
    .await;
    task.unwrap_or(1)
}

fn new_import_id() -> Result<String, GatewayError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| GatewayError::Random(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn import_failure(error: &OutboundImportError) -> ImportConfigurationResponse {
    response_error(error.code(), &error.to_string(), false)
}

fn credential_failure(message: &str, cleanup_failures: usize) -> ImportConfigurationResponse {
    let mut response = response_error("NP_CREDENTIAL_STORE_FAILED", message, true);
    append_cleanup_warning(&mut response, cleanup_failures);
    response
}

fn gateway_failure(error: GatewayError) -> ImportConfigurationResponse {
    response_error(error.code(), &error.to_string(), error.retryable())
}

fn response_error(code: &str, message: &str, retryable: bool) -> ImportConfigurationResponse {
    ImportConfigurationResponse {
        import_id: String::new(),
        outbounds: Vec::new(),
        warnings: Vec::new(),
        error: Some(ErrorDetail {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable,
            metadata: Default::default(),
        }),
    }
}

fn append_cleanup_warning(response: &mut ImportConfigurationResponse, cleanup_failures: usize) {
    append_warning(&mut response.warnings, cleanup_failures);
}

fn append_warning(warnings: &mut Vec<String>, cleanup_failures: usize) {
    if cleanup_failures > 0 {
        warnings.push("代理凭据未能全部清理，可在系统凭据库中手动删除未引用项。".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
    };

    use zeroize::Zeroizing;

    use super::store_new_credentials;
    use crate::{
        credential_store::{CredentialStore, CredentialStoreError},
        outbound_import::PreparedCredential,
    };

    #[tokio::test]
    async fn partial_credential_write_removes_every_successful_predecessor() {
        let store = Arc::new(FailingCredentialStore::default());
        let credentials = vec![credential("first", b"one"), credential("failure", b"two")];

        let result = store_new_credentials(store.clone(), credentials).await;

        assert_eq!(result, Err(0));
        assert!(store.references().is_empty());
    }

    fn credential(reference: &str, secret: &[u8]) -> PreparedCredential {
        PreparedCredential {
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
            if reference == "failure" {
                return Err(CredentialStoreError::Operation("测试写入"));
            }
            self.references
                .lock()
                .map_err(|_| CredentialStoreError::Operation("测试锁定"))?
                .insert(reference.to_owned());
            Ok(())
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
