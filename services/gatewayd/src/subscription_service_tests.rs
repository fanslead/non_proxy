use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nonproxy_outbound::ShadowsocksCredentials;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{MINIMUM_REFRESH_INTERVAL_SECONDS, PolicyDatabase};
use nonproxy_subscription::{SubscriptionEndpoint, SubscriptionFetchError};
use zeroize::Zeroizing;

use crate::{
    Gateway,
    credential_store::{CredentialStore, tests_support::MemoryCredentialStore},
    subscription_fetcher::SubscriptionFetcher,
    subscription_service::SubscriptionService,
    subscription_service_types::{SubscriptionServiceError, SubscriptionUpsert},
};

#[tokio::test]
async fn creates_source_and_rotates_password_without_changing_node_identity() {
    let first = subscription("Proxy.Example.", "first");
    let second = subscription("proxy.example", "second");
    let (gateway, store, service) = fixture(vec![Ok(first), Ok(second)]);

    let created = service
        .upsert_at(upsert("https://feed.example/nodes", None), 1_000)
        .await
        .unwrap_or_else(|error| panic!("订阅创建失败: {error}"));
    let initial = gateway
        .list_outbounds()
        .await
        .unwrap_or_else(|error| panic!("首次出口读取失败: {error}"));
    assert_eq!(created.generation, 1);
    assert_eq!(initial.len(), 1);
    let initial_id = initial[0].id().clone();
    let initial_reference = credential_reference(&initial[0]);
    assert_eq!(password(&store, &initial_reference), "first");

    let refreshed = service
        .refresh_at("office".to_owned(), 1, 2_000)
        .await
        .unwrap_or_else(|error| panic!("订阅密码轮换失败: {error}"));
    let current = gateway
        .list_outbounds()
        .await
        .unwrap_or_else(|error| panic!("轮换后出口读取失败: {error}"));
    assert_eq!(refreshed.generation, 2);
    assert_eq!(current[0].id(), &initial_id);
    assert_eq!(current[0].revision(), 2);
    let current_reference = credential_reference(&current[0]);
    assert_ne!(current_reference, initial_reference);
    assert!(!store.contains(&initial_reference));
    assert_eq!(password(&store, &current_reference), "second");
}

#[tokio::test]
async fn unchanged_refresh_keeps_generation_revision_and_credentials() {
    let payload = subscription("same.example", "private");
    let (gateway, store, service) = fixture(vec![Ok(payload.clone()), Ok(payload)]);
    service
        .upsert_at(upsert("https://feed.example/same", None), 1_000)
        .await
        .unwrap_or_else(|error| panic!("未变化场景订阅创建失败: {error}"));
    let initial = gateway
        .list_outbounds()
        .await
        .unwrap_or_else(|error| panic!("未变化场景首次出口读取失败: {error}"));
    let reference = credential_reference(&initial[0]);

    let result = service
        .refresh_at("office".to_owned(), 1, 2_000)
        .await
        .unwrap_or_else(|error| panic!("未变化订阅刷新失败: {error}"));
    let source = gateway
        .subscription_source("office".to_owned())
        .await
        .unwrap_or_else(|error| panic!("未变化订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("未变化订阅源不存在"));
    let current = gateway
        .list_outbounds()
        .await
        .unwrap_or_else(|error| panic!("未变化场景出口读取失败: {error}"));
    assert!(result.unchanged);
    assert_eq!(source.content_generation(), 1);
    assert_eq!(current[0].revision(), 1);
    assert_eq!(credential_reference(&current[0]), reference);
    assert!(store.contains(&reference));
}

#[tokio::test]
async fn failed_refresh_records_retry_state_without_changing_content() {
    let payload = subscription("failure.example", "private");
    let (gateway, _store, service) =
        fixture(vec![Ok(payload), Err(SubscriptionFetchError::Timeout)]);
    service
        .upsert_at(upsert("https://feed.example/failure", None), 1_000)
        .await
        .unwrap_or_else(|error| panic!("失败场景订阅创建失败: {error}"));

    let error = service
        .refresh_at("office".to_owned(), 1, 2_000)
        .await
        .err()
        .unwrap_or(SubscriptionServiceError::SourceNotFound);
    let source = gateway
        .subscription_source("office".to_owned())
        .await
        .unwrap_or_else(|read_error| panic!("失败后订阅源读取失败: {read_error}"))
        .unwrap_or_else(|| panic!("失败后订阅源不存在"));
    assert_eq!(error.code(), "NP_SUBSCRIPTION_TIMEOUT");
    assert_eq!(source.consecutive_failures(), 1);
    assert_eq!(source.last_error_code(), Some("NP_SUBSCRIPTION_TIMEOUT"));
    assert_eq!(source.next_refresh_at_unix_ms(), 62_000);
    assert_eq!(source.content_generation(), 1);
}

#[tokio::test]
async fn failed_url_reconfiguration_preserves_old_source_and_secret() {
    let payload = subscription("preserved.example", "private");
    let (gateway, store, service) =
        fixture(vec![Ok(payload), Err(SubscriptionFetchError::Connect)]);
    service
        .upsert_at(upsert("https://old.example/nodes?token=old", None), 1_000)
        .await
        .unwrap_or_else(|error| panic!("重配置场景订阅创建失败: {error}"));
    let before = gateway
        .subscription_source("office".to_owned())
        .await
        .unwrap_or_else(|error| panic!("重配置前订阅源读取失败: {error}"))
        .unwrap_or_else(|| panic!("重配置前订阅源不存在"));
    let reference = before.endpoint_credential().item_reference().to_owned();

    let error = service
        .upsert_at(
            upsert("https://new.example/nodes?token=new", Some(1)),
            2_000,
        )
        .await
        .err()
        .unwrap_or(SubscriptionServiceError::SourceNotFound);
    let after = gateway
        .subscription_source("office".to_owned())
        .await
        .unwrap_or_else(|read_error| panic!("重配置失败后订阅源读取失败: {read_error}"))
        .unwrap_or_else(|| panic!("重配置失败后订阅源不存在"));
    assert_eq!(error.code(), "NP_SUBSCRIPTION_CONNECT_FAILED");
    assert_eq!(after.revision(), 1);
    assert_eq!(after.endpoint_credential().item_reference(), reference);
    assert_eq!(
        store.value(&reference).as_deref(),
        Some(b"https://old.example/nodes?token=old".as_slice())
    );
}

fn fixture(
    responses: Vec<Result<Vec<u8>, SubscriptionFetchError>>,
) -> (Gateway, Arc<MemoryCredentialStore>, SubscriptionService) {
    let database = PolicyDatabase::open_in_memory(1_000)
        .unwrap_or_else(|error| panic!("订阅服务测试数据库创建失败: {error}"));
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let store = Arc::new(MemoryCredentialStore::default());
    let credential_store: Arc<dyn CredentialStore> = store.clone();
    let fetcher = Arc::new(FakeFetcher::new(responses));
    let service = SubscriptionService::new(gateway.clone(), credential_store, fetcher);
    (gateway, store, service)
}

fn upsert(endpoint: &str, expected_revision: Option<u64>) -> SubscriptionUpsert {
    SubscriptionUpsert {
        source_id: "office".to_owned(),
        display_name: "办公室订阅".to_owned(),
        endpoint_url: Zeroizing::new(endpoint.as_bytes().to_vec()),
        enabled: true,
        refresh_interval_seconds: MINIMUM_REFRESH_INTERVAL_SECONDS,
        expected_revision,
    }
}

fn subscription(host: &str, password: &str) -> Vec<u8> {
    let user_info = STANDARD.encode(format!("aes-256-gcm:{password}"));
    STANDARD
        .encode(format!("ss://{user_info}@{host}:8388#Office"))
        .into_bytes()
}

fn credential_reference(outbound: &nonproxy_storage::OutboundReference) -> String {
    outbound
        .credential()
        .map(nonproxy_storage::CredentialReference::item_reference)
        .unwrap_or_else(|| panic!("订阅 Shadowsocks 出口缺少凭据引用"))
        .to_owned()
}

fn password(store: &MemoryCredentialStore, reference: &str) -> String {
    let encoded = store
        .value(reference)
        .unwrap_or_else(|| panic!("测试凭据不存在: {reference}"));
    let credential = ShadowsocksCredentials::decode(&encoded)
        .unwrap_or_else(|error| panic!("测试 Shadowsocks 凭据解码失败: {error}"));
    let debug = format!("{credential:?}");
    assert!(!debug.contains("first") && !debug.contains("second"));
    let encoded = credential.encode();
    let method_length = usize::from(encoded[1]);
    String::from_utf8(encoded[(2 + method_length)..].to_vec())
        .unwrap_or_else(|error| panic!("测试密码不是 UTF-8: {error}"))
}

struct FakeFetcher {
    responses: Mutex<VecDeque<Result<Vec<u8>, SubscriptionFetchError>>>,
}

impl FakeFetcher {
    fn new(responses: Vec<Result<Vec<u8>, SubscriptionFetchError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

#[tonic::async_trait]
impl SubscriptionFetcher for FakeFetcher {
    async fn fetch(
        &self,
        _endpoint: &SubscriptionEndpoint,
    ) -> Result<Zeroizing<Vec<u8>>, SubscriptionFetchError> {
        self.responses
            .lock()
            .map_err(|_| SubscriptionFetchError::Http)?
            .pop_front()
            .unwrap_or(Err(SubscriptionFetchError::Http))
            .map(Zeroizing::new)
    }
}
