use nonproxy_model::{
    AppMatcher, DecisionSpec, FailureMode, OutboundId, Platform, Policy, PolicyId, PolicyMatch,
    PolicyMetadata, PolicyOrigin, PolicySourceKind, RouteAction,
};
use nonproxy_storage::{
    CredentialKind, CredentialReference, OutboundKind, OutboundReference, PolicyDatabase,
    StorageError,
};

fn outbound(revision: u64) -> OutboundReference {
    let id = match OutboundId::new("primary-socks") {
        Ok(value) => value,
        Err(error) => panic!("测试出口标识创建失败: {error}"),
    };
    let credential = match CredentialReference::new(
        "keychain:primary-socks",
        CredentialKind::Password,
        "主代理凭据",
        revision,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试凭据引用创建失败: {error}"),
    };
    match OutboundReference::new(
        id,
        OutboundKind::Socks5,
        Some("PROXY.Example.COM."),
        Some(1080),
        Some(credential),
        revision,
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试出口配置创建失败: {error}"),
    }
}

#[test]
fn outbound_round_trip_keeps_only_credential_reference_metadata() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let outbound = outbound(1);
    if let Err(error) = database.outbounds().save(&outbound, None, 1_100) {
        panic!("出口配置保存失败: {error}");
    }
    let loaded = database.outbounds().get(outbound.id());
    let Ok(Some(loaded)) = loaded else {
        panic!("出口配置读取失败: {loaded:?}");
    };

    assert_eq!(loaded, outbound);
    assert_eq!(loaded.endpoint_host(), Some("proxy.example.com"));
    assert_eq!(
        loaded.credential().map(CredentialReference::item_reference),
        Some("keychain:primary-socks")
    );
}

#[test]
fn secret_bearing_uri_is_rejected_as_an_endpoint() {
    let id = match OutboundId::new("unsafe") {
        Ok(value) => value,
        Err(error) => panic!("测试出口标识创建失败: {error}"),
    };

    assert!(matches!(
        OutboundReference::new(
            id,
            OutboundKind::HttpConnect,
            Some("https://user:password@example.com"),
            Some(443),
            None,
            1,
        ),
        Err(StorageError::OutboundInvalid)
    ));
}

#[test]
fn outbound_revision_is_optimistic_and_proxy_policy_can_reference_it() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let initial = outbound(1);
    if let Err(error) = database.outbounds().save(&initial, None, 1_100) {
        panic!("初始出口保存失败: {error}");
    }
    let updated = outbound(2);
    if let Err(error) = database.outbounds().save(&updated, Some(1), 1_200) {
        panic!("出口更新失败: {error}");
    }
    assert!(matches!(
        database.outbounds().save(&updated, Some(1), 1_300),
        Err(StorageError::OutboundRevisionConflict)
    ));

    let policy = proxy_policy(initial.id().clone());
    if let Err(error) = database.policies().save(&policy, None, 1_400) {
        panic!("代理策略保存失败: {error}");
    }
    let loaded = database.policies().get(policy.id());
    let Ok(Some(loaded)) = loaded else {
        panic!("代理策略读取失败: {loaded:?}");
    };
    assert_eq!(loaded, policy);
}

fn proxy_policy(outbound_id: OutboundId) -> Policy {
    let app = match AppMatcher::new(Platform::MacOs, "com.example.proxy") {
        Ok(value) => value,
        Err(error) => panic!("测试应用匹配器创建失败: {error}"),
    };
    let matcher = match PolicyMatch::new(Some(app), None, None, None, Vec::new(), Vec::new()) {
        Ok(value) => value,
        Err(error) => panic!("测试策略匹配器创建失败: {error}"),
    };
    let decision =
        match DecisionSpec::new(RouteAction::Proxy, Some(outbound_id), FailureMode::Closed) {
            Ok(value) => value,
            Err(error) => panic!("测试代理决策创建失败: {error}"),
        };
    let id = match PolicyId::new("proxy-policy") {
        Ok(value) => value,
        Err(error) => panic!("测试策略标识创建失败: {error}"),
    };
    match Policy::new(
        id,
        "代理策略",
        matcher,
        decision,
        PolicyMetadata::new(PolicySourceKind::App, 0, PolicyOrigin::User, 1),
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试代理策略创建失败: {error}"),
    }
}
