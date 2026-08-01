use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{MINIMUM_REFRESH_INTERVAL_SECONDS, PolicyDatabase};
use nonproxy_subscription::{SubscriptionEndpoint, SubscriptionFetchError};
use zeroize::Zeroizing;

use crate::{
    Gateway,
    credential_store::{CredentialStore, CredentialStoreError},
    subscription_fetcher::SubscriptionFetcher,
    subscription_service::SubscriptionService,
    subscription_service_types::SubscriptionUpsert,
};

#[tokio::test]
async fn delete_retries_durable_credential_cleanup_without_restoring_database_state() {
    let database = PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("删除测试数据库创建失败: {error}"));
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let store = Arc::new(RetryingCredentialStore::default());
    let credential_store: Arc<dyn CredentialStore> = store.clone();
    let fetcher = Arc::new(StaticFetcher(subscription("delete.example", "private")));
    let service = SubscriptionService::new(gateway.clone(), credential_store, fetcher);
    service
        .upsert_at(upsert(), 1_000)
        .await
        .unwrap_or_else(|error| panic!("删除测试订阅创建失败: {error}"));
    assert_eq!(store.count(), 2);

    store.fail_deletes.store(true, Ordering::SeqCst);
    let deleted = service
        .delete_at("office".to_owned(), 1, 2_000)
        .await
        .unwrap_or_else(|error| panic!("订阅删除失败: {error}"));
    assert_eq!(deleted.outbound_count, 1);
    assert_eq!(deleted.cleanup_failures, 2);
    assert!(
        gateway
            .subscription_source("office".to_owned())
            .await
            .unwrap_or_else(|error| panic!("删除后订阅读取失败: {error}"))
            .is_none()
    );
    assert!(
        gateway
            .list_outbounds()
            .await
            .unwrap_or_else(|error| panic!("删除后出口读取失败: {error}"))
            .is_empty()
    );
    assert_eq!(store.count(), 2);
    assert!(
        gateway
            .due_credential_cleanup(61_999, 100)
            .await
            .unwrap_or_else(|error| panic!("提前读取清理队列失败: {error}"))
            .is_empty()
    );
    assert_eq!(
        gateway
            .due_credential_cleanup(62_000, 100)
            .await
            .unwrap_or_else(|error| panic!("到期清理队列读取失败: {error}"))
            .len(),
        2
    );

    store.fail_deletes.store(false, Ordering::SeqCst);
    assert_eq!(
        service
            .retry_credential_cleanup_at(62_000)
            .await
            .unwrap_or_else(|error| panic!("凭据清理重试失败: {error}")),
        0
    );
    assert_eq!(store.count(), 0);
    assert!(
        gateway
            .due_credential_cleanup(i64::MAX as u64, 100)
            .await
            .unwrap_or_else(|error| panic!("重试后清理队列读取失败: {error}"))
            .is_empty()
    );
}

fn upsert() -> SubscriptionUpsert {
    SubscriptionUpsert {
        source_id: "office".to_owned(),
        display_name: "办公室订阅".to_owned(),
        endpoint_url: Some(Zeroizing::new(b"https://feed.example/delete".to_vec())),
        enabled: true,
        refresh_interval_seconds: MINIMUM_REFRESH_INTERVAL_SECONDS,
        expected_revision: None,
    }
}

fn subscription(host: &str, password: &str) -> Vec<u8> {
    let user_info = STANDARD.encode(format!("aes-256-gcm:{password}"));
    STANDARD
        .encode(format!("ss://{user_info}@{host}:8388#Office"))
        .into_bytes()
}

struct StaticFetcher(Vec<u8>);

#[tonic::async_trait]
impl SubscriptionFetcher for StaticFetcher {
    async fn fetch(
        &self,
        _endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError> {
        Ok(Zeroizing::new(self.0.clone()))
    }
}

#[derive(Default)]
struct RetryingCredentialStore {
    entries: Mutex<HashMap<String, Vec<u8>>>,
    fail_deletes: AtomicBool,
}

impl RetryingCredentialStore {
    fn count(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }
}

impl CredentialStore for RetryingCredentialStore {
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
        if self.fail_deletes.load(Ordering::SeqCst) {
            return Err(CredentialStoreError::Operation("删除"));
        }
        self.entries
            .lock()
            .map_err(|_| CredentialStoreError::Operation("锁定"))?
            .remove(reference);
        Ok(())
    }
}
