use nonproxy_model::{
    DecisionSpec, NetworkMatcher, NetworkProfileId, Policy, PolicyId, PolicyMatch, PolicyMetadata,
    PolicyOrigin, PolicySourceKind,
};
use nonproxy_storage::{
    NetworkFingerprint, NetworkFingerprintKind, NetworkProfileReference, PolicyDatabase,
    StorageError,
};

fn profile(revision: u64) -> NetworkProfileReference {
    let id = match NetworkProfileId::new("office-network") {
        Ok(value) => value,
        Err(error) => panic!("测试网络画像标识创建失败: {error}"),
    };
    let fingerprint =
        match NetworkFingerprint::new(NetworkFingerprintKind::WifiSsidSha256, "a".repeat(64)) {
            Ok(value) => value,
            Err(error) => panic!("测试网络指纹创建失败: {error}"),
        };
    match NetworkProfileReference::new(id, "办公室网络", fingerprint, revision) {
        Ok(value) => value,
        Err(error) => panic!("测试网络画像创建失败: {error}"),
    }
}

#[test]
fn hashed_network_profile_round_trip_supports_network_policy() {
    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let profile = profile(1);
    if let Err(error) = database.network_profiles().save(&profile, None, 1_100) {
        panic!("网络画像保存失败: {error}");
    }
    let loaded = database.network_profiles().get(profile.id());
    let Ok(Some(loaded)) = loaded else {
        panic!("网络画像读取失败: {loaded:?}");
    };
    assert_eq!(loaded, profile);

    let policy = network_policy(profile.id().clone());
    if let Err(error) = database.policies().save(&policy, None, 1_200) {
        panic!("网络策略保存失败: {error}");
    }
    let loaded_policy = database.policies().get(policy.id());
    let Ok(Some(loaded_policy)) = loaded_policy else {
        panic!("网络策略读取失败: {loaded_policy:?}");
    };
    assert_eq!(loaded_policy, policy);
}

#[test]
fn raw_wifi_name_and_stale_revision_are_rejected() {
    assert!(matches!(
        NetworkFingerprint::new(NetworkFingerprintKind::WifiSsidSha256, "Office WiFi",),
        Err(StorageError::NetworkProfileInvalid)
    ));

    let database = PolicyDatabase::open_in_memory(1_000);
    let Ok(mut database) = database else {
        panic!("测试数据库打开失败: {database:?}");
    };
    let initial = profile(1);
    if let Err(error) = database.network_profiles().save(&initial, None, 1_100) {
        panic!("初始网络画像保存失败: {error}");
    }
    let updated = profile(2);
    if let Err(error) = database.network_profiles().save(&updated, Some(1), 1_200) {
        panic!("网络画像更新失败: {error}");
    }

    assert!(matches!(
        database.network_profiles().save(&updated, Some(1), 1_300),
        Err(StorageError::NetworkProfileRevisionConflict)
    ));
}

fn network_policy(profile_id: NetworkProfileId) -> Policy {
    let matcher = match PolicyMatch::new(
        None,
        None,
        None,
        Some(NetworkMatcher::new(profile_id)),
        Vec::new(),
        Vec::new(),
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试网络策略匹配器创建失败: {error}"),
    };
    let id = match PolicyId::new("office-network-policy") {
        Ok(value) => value,
        Err(error) => panic!("测试网络策略标识创建失败: {error}"),
    };
    match Policy::new(
        id,
        "办公室网络直连",
        matcher,
        DecisionSpec::direct(),
        PolicyMetadata::new(PolicySourceKind::Network, 0, PolicyOrigin::User, 1),
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试网络策略创建失败: {error}"),
    }
}
