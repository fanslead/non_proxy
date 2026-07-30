#![allow(dead_code)]

use nonproxy_model::{
    AppMatcher, DecisionSpec, DomainMatchKind, DomainMatcher, Platform, Policy, PolicyId,
    PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind, PortRange, Transport,
};
use nonproxy_storage::{SnapshotArtifact, StorageError};

pub fn must_policy(revision: u64, display_name: &str) -> Policy {
    let app = match AppMatcher::new(Platform::MacOs, "com.example.app")
        .and_then(|value| value.with_signer_id("TEAM1"))
    {
        Ok(value) => value.include_helpers(true),
        Err(error) => panic!("测试应用匹配器创建失败: {error}"),
    };
    let domain = match DomainMatcher::new(DomainMatchKind::Suffix, "example.com") {
        Ok(value) => value,
        Err(error) => panic!("测试域名匹配器创建失败: {error}"),
    };
    let port = match PortRange::new(443, 443) {
        Ok(value) => value,
        Err(error) => panic!("测试端口范围创建失败: {error}"),
    };
    let matcher = match PolicyMatch::new(
        Some(app),
        Some(domain),
        None,
        None,
        vec![Transport::Tcp],
        vec![port],
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试策略匹配器创建失败: {error}"),
    };
    let id = match PolicyId::new("policy-app-site") {
        Ok(value) => value,
        Err(error) => panic!("测试策略标识创建失败: {error}"),
    };
    match Policy::new(
        id,
        display_name,
        matcher,
        DecisionSpec::direct(),
        PolicyMetadata::new(
            PolicySourceKind::AppDestination,
            12,
            PolicyOrigin::User,
            revision,
        ),
    ) {
        Ok(value) => value,
        Err(error) => panic!("测试策略创建失败: {error}"),
    }
}

pub fn artifact(version: u64, marker: u8) -> Result<SnapshotArtifact, StorageError> {
    SnapshotArtifact::new(
        version,
        1,
        1_000 + version,
        [marker; 32],
        usize::from(marker),
        vec![marker, marker.saturating_add(1)],
    )
}
