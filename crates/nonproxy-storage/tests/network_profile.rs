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
    assert!(
        NetworkFingerprint::new(NetworkFingerprintKind::WifiSsidSha256, "Office WiFi").is_err()
    );

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

#[test]
fn catalog_is_ordered_unique_and_revisioned() {
    let Ok(mut database) = PolicyDatabase::open_in_memory(1_000) else {
        panic!("网络配置档目录测试数据库打开失败");
    };
    let office = profile(1);
    let home = NetworkProfileReference::new(
        NetworkProfileId::new("home-network")
            .unwrap_or_else(|error| panic!("家庭网络标识创建失败: {error}")),
        "家庭网络",
        NetworkFingerprint::new(NetworkFingerprintKind::DefaultGatewaySha256, "b".repeat(64))
            .unwrap_or_else(|error| panic!("家庭网络指纹创建失败: {error}")),
        1,
    )
    .unwrap_or_else(|error| panic!("家庭网络配置档创建失败: {error}"));

    database
        .network_profiles()
        .save(&office, None, 1_100)
        .unwrap_or_else(|error| panic!("办公室网络保存失败: {error}"));
    database
        .network_profiles()
        .save(&home, None, 1_200)
        .unwrap_or_else(|error| panic!("家庭网络保存失败: {error}"));

    let listed = database
        .network_profiles()
        .list()
        .unwrap_or_else(|error| panic!("网络配置档目录读取失败: {error}"));
    assert_eq!(
        listed
            .iter()
            .map(|value| value.id().as_str())
            .collect::<Vec<_>>(),
        vec!["home-network", "office-network"]
    );
    assert_eq!(
        database
            .network_profiles()
            .catalog_generation()
            .unwrap_or_else(|error| panic!("网络配置档目录代数读取失败: {error}")),
        2
    );

    let duplicate = NetworkProfileReference::new(
        NetworkProfileId::new("duplicate-network")
            .unwrap_or_else(|error| panic!("重复网络标识创建失败: {error}")),
        "重复网络",
        office.fingerprint().clone(),
        1,
    )
    .unwrap_or_else(|error| panic!("重复网络配置档创建失败: {error}"));
    assert!(matches!(
        database.network_profiles().save(&duplicate, None, 1_300),
        Err(StorageError::NetworkProfileFingerprintConflict)
    ));
}

#[test]
fn referenced_profile_cannot_be_deleted() {
    let Ok(mut database) = PolicyDatabase::open_in_memory(1_000) else {
        panic!("网络配置档删除测试数据库打开失败");
    };
    let profile = profile(1);
    database
        .network_profiles()
        .save(&profile, None, 1_100)
        .unwrap_or_else(|error| panic!("删除测试网络配置档保存失败: {error}"));
    let policy = network_policy(profile.id().clone());
    database
        .policies()
        .save(&policy, None, 1_200)
        .unwrap_or_else(|error| panic!("删除测试网络策略保存失败: {error}"));

    assert!(matches!(
        database.network_profiles().delete(profile.id(), 1, 1_300),
        Err(StorageError::NetworkProfileInUse)
    ));

    database
        .policies()
        .delete(policy.id(), 1, 1_400)
        .unwrap_or_else(|error| panic!("删除测试网络策略删除失败: {error}"));
    database
        .network_profiles()
        .delete(profile.id(), 1, 1_500)
        .unwrap_or_else(|error| panic!("网络配置档删除失败: {error}"));
    assert!(
        database
            .network_profiles()
            .get(profile.id())
            .unwrap_or_else(|error| panic!("删除后网络配置档读取失败: {error}"))
            .is_none()
    );
    assert_eq!(
        database
            .network_profiles()
            .catalog_generation()
            .unwrap_or_else(|error| panic!("删除后目录代数读取失败: {error}")),
        2
    );
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
