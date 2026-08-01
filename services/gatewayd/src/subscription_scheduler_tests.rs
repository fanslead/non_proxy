use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{MINIMUM_REFRESH_INTERVAL_SECONDS, PolicyDatabase};
use nonproxy_subscription::{SubscriptionEndpoint, SubscriptionFetchError};
use tokio::{sync::Semaphore, task::JoinSet};
use zeroize::Zeroizing;

use crate::{
    Gateway,
    credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
    subscription_fetcher::SubscriptionFetcher,
    subscription_scheduler::{SubscriptionScheduler, remove_completed},
    subscription_service::SubscriptionService,
    subscription_service_types::SubscriptionUpsert,
};

#[tokio::test]
async fn schedules_only_four_distinct_due_sources_per_batch() {
    let database = PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("调度器测试数据库创建失败: {error}"));
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let store: Arc<dyn CredentialStore> = Arc::new(MemoryCredentialStore::default());
    let fetcher = Arc::new(GatedFetcher::new(subscription()));
    let service = SubscriptionService::new(gateway.clone(), store, fetcher.clone());
    for index in 0..5 {
        service
            .upsert_at(upsert(index), 1_000)
            .await
            .unwrap_or_else(|error| panic!("调度器测试订阅创建失败: {error}"));
    }
    fetcher.block();
    let scheduler = SubscriptionScheduler::new(gateway, service);
    let mut tasks = JoinSet::new();
    let mut active = std::collections::HashSet::new();

    scheduler
        .schedule_due(1_000_000_000, &mut tasks, &mut active)
        .await;
    fetcher.wait_for_calls(9).await;
    assert_eq!(tasks.len(), 4);
    assert_eq!(active.len(), 4);

    fetcher.release(4);
    while let Some(completion) = tasks.join_next().await {
        remove_completed(Some(completion), &tasks, &mut active);
    }
    scheduler
        .schedule_due(1_000_000_000, &mut tasks, &mut active)
        .await;
    fetcher.wait_for_calls(10).await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(active.len(), 1);
    fetcher.release(1);
    while let Some(completion) = tasks.join_next().await {
        remove_completed(Some(completion), &tasks, &mut active);
    }
    assert!(active.is_empty());
}

fn upsert(index: usize) -> SubscriptionUpsert {
    SubscriptionUpsert {
        source_id: format!("source-{index}"),
        display_name: format!("订阅 {index}"),
        endpoint_url: Some(Zeroizing::new(
            format!("https://feed.example/{index}").into_bytes(),
        )),
        enabled: true,
        refresh_interval_seconds: MINIMUM_REFRESH_INTERVAL_SECONDS,
        expected_revision: None,
    }
}

fn subscription() -> Vec<u8> {
    let user_info = STANDARD.encode("aes-256-gcm:private");
    STANDARD
        .encode(format!("ss://{user_info}@proxy.example:8388#Office"))
        .into_bytes()
}

struct GatedFetcher {
    payload: Vec<u8>,
    blocked: AtomicBool,
    calls: AtomicUsize,
    permits: Semaphore,
}

impl GatedFetcher {
    fn new(payload: Vec<u8>) -> Self {
        Self {
            payload,
            blocked: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            permits: Semaphore::new(0),
        }
    }

    fn block(&self) {
        self.blocked.store(true, Ordering::SeqCst);
    }

    fn release(&self, permits: usize) {
        self.permits.add_permits(permits);
    }

    async fn wait_for_calls(&self, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while self.calls.load(Ordering::SeqCst) < expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("等待调度器请求超时: {error}"));
    }
}

#[tonic::async_trait]
impl SubscriptionFetcher for GatedFetcher {
    async fn fetch(
        &self,
        _endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.blocked.load(Ordering::SeqCst) {
            self.permits
                .acquire()
                .await
                .map_err(|_| SubscriptionFetchError::Http)?
                .forget();
        }
        Ok(Zeroizing::new(self.payload.clone()))
    }
}
