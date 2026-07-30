use nonproxy_proto::{
    common::v1::{ComponentKind, ComponentVersion},
    provider::v1::{ProviderKind, RegisterProviderRequest},
};

use super::{REQUIRED_CAPABILITIES, validate_registration};
use crate::provider_session::PROVIDER_SESSION_LIFETIME_MS;

#[test]
fn registration_requires_matching_component_and_protocol() {
    let request = registration(ComponentKind::DnsProxy, 0, 0);

    assert!(validate_registration(&request).is_err());
    assert_eq!(PROVIDER_SESSION_LIFETIME_MS, 900_000);
}

#[test]
fn registration_rejects_protocol_range_without_server_minor() {
    let request = registration(ComponentKind::TransparentProxy, 2, 1);

    assert!(validate_registration(&request).is_err());
}

#[test]
fn registration_rejects_missing_or_duplicate_capabilities() {
    let mut request = registration(ComponentKind::TransparentProxy, 0, 0);
    request.capabilities = vec!["snapshot-v1".to_owned()];
    assert!(validate_registration(&request).is_err());

    request.capabilities = vec![
        "snapshot-v1".to_owned(),
        "heartbeat-v1".to_owned(),
        "heartbeat-v1".to_owned(),
    ];
    assert!(validate_registration(&request).is_err());
}

fn registration(
    component: ComponentKind,
    protocol_minor: u32,
    minimum_protocol_minor: u32,
) -> RegisterProviderRequest {
    RegisterProviderRequest {
        provider_instance_id: "transparent-1".to_owned(),
        kind: ProviderKind::TransparentProxy as i32,
        version: Some(ComponentVersion {
            component: component as i32,
            semantic_version: "1.0.0".to_owned(),
            build_id: "test".to_owned(),
            protocol_major: 1,
            protocol_minor,
            minimum_protocol_minor,
        }),
        capabilities: REQUIRED_CAPABILITIES
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        startup_nonce: vec![1; 32],
        bootstrap_capability: vec![2; 32],
    }
}
