use std::fs;

use nonproxy_model::{AppIdentity, Decision, DecisionSpec, Destination, Platform, Transport};
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_proto::control::v1::{DiagnosticRedactionLevel, ExportDiagnosticsRequest};
use nonproxy_storage::{ConnectionDecisionInput, DecisionEvidence, EvidenceLevel, PolicyDatabase};
use sha2::{Digest, Sha256};

use crate::{Gateway, diagnostics_export};

const SECRET_APP: &str = "com.private.browser";
const SECRET_DOMAIN: &str = "payroll.private.example";

#[tokio::test]
async fn strict_export_is_private_bounded_and_hash_verified() {
    let gateway = gateway_with_sensitive_decision().await;
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("诊断测试临时目录创建失败: {error}"));

    let exported = diagnostics_export::export(
        &gateway,
        &directory.path().join("diagnostics"),
        request(DiagnosticRedactionLevel::Strict),
    )
    .await
    .unwrap_or_else(|error| panic!("严格诊断导出失败: {error:?}"));
    let response = exported
        .into_response()
        .unwrap_or_else(|error| panic!("严格诊断响应生成失败: {error}"));
    let content = fs::read(&response.local_path)
        .unwrap_or_else(|error| panic!("严格诊断文件读取失败: {error}"));
    let json: serde_json::Value = serde_json::from_slice(&content)
        .unwrap_or_else(|error| panic!("严格诊断 JSON 无效: {error}"));

    assert_eq!(response.connection_sample_count, 0);
    assert_eq!(response.error_count, 1);
    assert_eq!(response.sha256, Sha256::digest(&content).to_vec());
    assert!(!String::from_utf8_lossy(&content).contains(SECRET_APP));
    assert!(!String::from_utf8_lossy(&content).contains(SECRET_DOMAIN));
    assert_eq!(json["redaction"]["level"], "strict");
    assert_eq!(json["connection_samples"].as_array().map(Vec::len), Some(0));
    assert_eq!(json["redaction"]["credentials_included"], false);
    assert_eq!(json["redaction"]["endpoints_included"], false);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let permissions = fs::metadata(&response.local_path)
            .unwrap_or_else(|error| panic!("严格诊断权限读取失败: {error}"))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(permissions, 0o600);
    }
}

#[tokio::test]
async fn standard_export_uses_per_export_pseudonyms_without_raw_identifiers() {
    let gateway = gateway_with_sensitive_decision().await;
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("标准诊断测试临时目录创建失败: {error}"));

    let first = export_content(&gateway, directory.path()).await;
    let second = export_content(&gateway, directory.path()).await;
    let first_text = String::from_utf8_lossy(&first);

    assert!(!first_text.contains(SECRET_APP));
    assert!(!first_text.contains(SECRET_DOMAIN));
    assert!(first_text.contains("app-"));
    assert!(first_text.contains("target-"));
    assert_ne!(first, second);
}

#[tokio::test]
async fn export_rejects_non_directory_destination_without_writing() {
    let gateway = Gateway::new(
        PolicyDatabase::open_in_memory(1)
            .unwrap_or_else(|error| panic!("路径测试数据库打开失败: {error}")),
        CompileCapabilities::full(),
    );
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("路径测试临时目录创建失败: {error}"));
    let occupied = directory.path().join("diagnostics");
    fs::write(&occupied, b"occupied")
        .unwrap_or_else(|error| panic!("路径测试占位文件写入失败: {error}"));

    let result = diagnostics_export::export(
        &gateway,
        &occupied,
        request(DiagnosticRedactionLevel::Strict),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(
        fs::read(&occupied).ok().as_deref(),
        Some(b"occupied".as_slice())
    );
}

async fn gateway_with_sensitive_decision() -> Gateway {
    let database = PolicyDatabase::open_in_memory(1)
        .unwrap_or_else(|error| panic!("诊断测试数据库打开失败: {error}"));
    let gateway = Gateway::new(database, CompileCapabilities::full());
    let app = AppIdentity::new(Platform::MacOs, SECRET_APP)
        .and_then(|value| value.with_display_name("Private Browser"))
        .unwrap_or_else(|error| panic!("诊断测试应用身份无效: {error}"));
    let destination = Destination::new(Some(SECRET_DOMAIN), None, 443, Transport::Tcp)
        .unwrap_or_else(|error| panic!("诊断测试目标无效: {error}"));
    let decision = Decision::defaulted(DecisionSpec::direct(), 1, "NP_POLICY_DEFAULT");
    let evidence = DecisionEvidence::new(EvidenceLevel::Decision, None, None, None, false)
        .unwrap_or_else(|error| panic!("诊断测试证据无效: {error}"));
    let input = ConnectionDecisionInput::new(
        "macos-transparent-proxy",
        1,
        "diagnostic-flow",
        current_time_ms(),
        app,
        destination,
        decision,
        evidence,
        Some(1_000),
        Some("NP_TEST_PRIVATE_FAILURE".to_owned()),
    )
    .unwrap_or_else(|error| panic!("诊断测试决策无效: {error}"));
    gateway
        .store_connection_decisions(vec![input])
        .await
        .unwrap_or_else(|error| panic!("诊断测试决策保存失败: {error}"));
    gateway
}

async fn export_content(gateway: &Gateway, root: &std::path::Path) -> Vec<u8> {
    let exported = diagnostics_export::export(
        gateway,
        &root.join("diagnostics"),
        request(DiagnosticRedactionLevel::Standard),
    )
    .await
    .unwrap_or_else(|error| panic!("标准诊断导出失败: {error:?}"));
    let response = exported
        .into_response()
        .unwrap_or_else(|error| panic!("标准诊断响应生成失败: {error}"));
    assert_eq!(response.connection_sample_count, 1);
    fs::read(response.local_path).unwrap_or_else(|error| panic!("标准诊断文件读取失败: {error}"))
}

fn request(redaction_level: DiagnosticRedactionLevel) -> ExportDiagnosticsRequest {
    ExportDiagnosticsRequest {
        context: None,
        redaction_level: redaction_level as i32,
        time_range: None,
    }
}

fn current_time_ms() -> u64 {
    crate::clock::unix_time_ms().unwrap_or_else(|error| panic!("诊断测试系统时间读取失败: {error}"))
}
