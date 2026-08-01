use nonproxy_model::OutboundId;
use nonproxy_outbound::ShadowsocksCredentials;
use nonproxy_storage::{OutboundKind, OutboundReference};

use crate::outbound_import::{IMPORT_FORMAT, URI_LIST_IMPORT_FORMAT, prepare};

#[test]
fn prepares_versioned_credential_without_storing_secret_in_metadata() {
    let configuration = br#"{
        "version": 1,
        "outbounds": [{
            "id": "primary",
            "kind": "socks5",
            "host": "Proxy.Example.com.",
            "port": 1080,
            "username": "alice",
            "password": "private"
        }]
    }"#;

    let prepared = prepare(
        IMPORT_FORMAT,
        configuration,
        "00112233445566778899aabbccddeeff".to_owned(),
        &[],
    )
    .unwrap_or_else(|error| panic!("出口导入准备失败: {error}"));

    assert_eq!(prepared.outbounds.len(), 1);
    assert_eq!(prepared.credentials.len(), 1);
    let outbound = &prepared.outbounds[0].0;
    assert_eq!(outbound.endpoint_host(), Some("proxy.example.com"));
    let reference = outbound
        .credential()
        .map(nonproxy_storage::CredentialReference::item_reference);
    assert!(reference.is_some_and(|value| !value.contains("private")));
    assert_eq!(
        prepared.credentials[0].secret.as_slice(),
        b"\x01\x05aliceprivate"
    );
}

#[test]
fn rejects_unknown_fields_duplicate_ids_and_partial_credentials() {
    let cases: [&[u8]; 3] = [
        br#"{"version":1,"extra":true,"outbounds":[]}"#,
        br#"{"version":1,"outbounds":[
            {"id":"same","kind":"socks5","host":"a.example","port":1},
            {"id":"same","kind":"socks5","host":"b.example","port":2}
        ]}"#,
        br#"{"version":1,"outbounds":[{
            "id":"partial","kind":"http_connect",
            "host":"a.example","port":8080,"username":"alice"
        }]}"#,
    ];

    for configuration in cases {
        assert!(
            prepare(
                IMPORT_FORMAT,
                configuration,
                "00112233445566778899aabbccddeeff".to_owned(),
                &[],
            )
            .is_err()
        );
    }
}

#[test]
fn prepares_shadowsocks_key_without_leaking_it_into_metadata() {
    let configuration = br#"{
        "version": 1,
        "outbounds": [{
            "id": "modern-proxy",
            "kind": "shadowsocks",
            "host": "ss.example",
            "port": 8388,
            "method": "aes-256-gcm",
            "password": "private"
        }]
    }"#;

    let prepared = prepare(
        IMPORT_FORMAT,
        configuration,
        "00112233445566778899aabbccddeeff".to_owned(),
        &[],
    )
    .unwrap_or_else(|error| panic!("Shadowsocks 导入准备失败: {error}"));

    assert_eq!(prepared.outbounds[0].0.kind(), OutboundKind::Shadowsocks);
    assert_eq!(prepared.credentials.len(), 1);
    let decoded = ShadowsocksCredentials::decode(prepared.credentials[0].secret.as_slice())
        .unwrap_or_else(|error| panic!("Shadowsocks 导入密钥解码失败: {error}"));
    assert_eq!(decoded.method_name(), "aes-256-gcm");
    let metadata = format!("{:?}", prepared.outbounds[0].0);
    assert!(!metadata.contains("private"));

    let invalid = br#"{"version":1,"outbounds":[{
        "id":"modern-proxy","kind":"shadowsocks","host":"ss.example",
        "port":8388,"method":"aes-256-cfb","password":"private"
    }]}"#;
    assert!(
        prepare(
            IMPORT_FORMAT,
            invalid,
            "00112233445566778899aabbccddeeff".to_owned(),
            &[],
        )
        .is_err()
    );
}

#[test]
fn uri_preview_warns_before_replacing_an_existing_identifier() {
    let existing = OutboundReference::new(
        OutboundId::new("office").unwrap_or_else(|error| panic!("现有出口标识创建失败: {error}")),
        OutboundKind::Socks5,
        Some("old.example"),
        Some(1_080),
        None,
        7,
    )
    .unwrap_or_else(|error| panic!("现有出口配置创建失败: {error}"));

    let prepared = prepare(
        URI_LIST_IMPORT_FORMAT,
        b"socks5://new.example:1080#office",
        "00112233445566778899aabbccddeeff".to_owned(),
        &[existing],
    )
    .unwrap_or_else(|error| panic!("替换预检失败: {error}"));

    assert_eq!(prepared.outbounds[0].1, Some(7));
    assert_eq!(prepared.outbounds[0].0.revision(), 8);
    assert!(
        prepared
            .warnings
            .iter()
            .any(|value| value.contains("office 已存在") && value.contains("安全更新"))
    );
}
