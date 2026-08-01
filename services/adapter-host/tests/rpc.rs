#![cfg(unix)]

mod support;

use std::{fs, path::Path, time::Duration};

use hyper_util::rt::TokioIo;
use nonproxy_adapter_host::{AdapterHostConfig, AdapterRpcService};
use nonproxy_local_auth::SessionCapability;
use nonproxy_proto::{
    adapter::v1::{
        AdapterCapability, AdapterClient, AdapterRequestContext, ApplyChangeRequest,
        ListInstallationsRequest, PrepareChangeRequest, ReadCapabilitiesRequest,
        RegisterInstallationRequest, RemoveInstallationRequest, RollbackChangeRequest,
        VerifyChangeRequest, adapter_service_client::AdapterServiceClient,
        adapter_service_server::AdapterService,
    },
    common::v1::EvidenceLevel,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tonic::{Code, Request, transport::Endpoint};
use tower::service_fn;

use support::MockMihomoController;

const TOKEN: [u8; 32] = [7; 32];

#[tokio::test]
async fn authenticated_rpc_keeps_configuration_and_path_evidence_distinct() {
    let fixture = Fixture::new();
    let service = fixture.service();

    let registered = service
        .register_installation(Request::new(fixture.registration("register-1")))
        .await
        .unwrap_or_else(|error| panic!("安装项登记 RPC 失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    assert!(!registered.replayed);
    assert_eq!(
        registered
            .installation
            .as_ref()
            .map(|value| value.client_version.as_str()),
        Some("1.19.16")
    );

    let replay = service
        .register_installation(Request::new(fixture.registration("register-2")))
        .await
        .unwrap_or_else(|error| panic!("安装项登记重放失败: {error}"))
        .into_inner();
    assert!(replay.replayed);

    let capabilities = service
        .read_capabilities(Request::new(ReadCapabilitiesRequest {
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            context: Some(context("capabilities", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("能力读取 RPC 失败: {error}"))
        .into_inner();
    assert!(capabilities.error.is_none());
    assert_eq!(
        capabilities.capabilities,
        vec![
            AdapterCapability::AppRule as i32,
            AdapterCapability::DomainRule as i32,
            AdapterCapability::CidrRule as i32,
            AdapterCapability::HotReload as i32,
        ]
    );

    let policy = fixture.policy();
    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-1", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("变更准备 RPC 失败: {error}"))
        .into_inner();
    assert!(prepared.error.is_none());
    assert_eq!(prepared.rule_count, 1);
    assert!(prepared.client_validated);
    assert_eq!(prepared.configuration_candidate_hash.len(), 32);
    assert_eq!(prepared.managed_rules_reference, "./nonproxy.yaml");
    assert_eq!(prepared.direct_target, "DIRECT");

    let denied = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id.clone(),
            expected_candidate_hash: prepared.candidate_hash.clone(),
            context: Some(context("apply-denied", [0; 32])),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash.clone(),
        }))
        .await;
    assert!(matches!(denied, Err(status) if status.code() == Code::PermissionDenied));
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("拒绝后主配置读取失败: {error}")),
        fixture.initial_configuration()
    );

    let wrong_configuration_hash = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id.clone(),
            expected_candidate_hash: prepared.candidate_hash.clone(),
            context: Some(context("apply-wrong-configuration-hash", TOKEN)),
            expected_configuration_candidate_hash: vec![9; 32],
        }))
        .await
        .unwrap_or_else(|error| panic!("错误主配置哈希应用 RPC 失败: {error}"))
        .into_inner();
    assert!(!wrong_configuration_hash.applied);
    assert_eq!(
        wrong_configuration_hash
            .error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("NP_ADAPTER_CANDIDATE_HASH_MISMATCH")
    );
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("错误哈希后主配置读取失败: {error}")),
        fixture.initial_configuration()
    );

    let applied = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id.clone(),
            expected_candidate_hash: prepared.candidate_hash.clone(),
            context: Some(context("apply-1", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash.clone(),
        }))
        .await
        .unwrap_or_else(|error| panic!("变更应用 RPC 失败: {error}"))
        .into_inner();
    assert!(applied.applied);
    assert!(applied.reloaded);
    assert!(!applied.rolled_back);
    assert!(!applied.rollback_reloaded);
    assert!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("已应用主配置读取失败: {error}"))
            .contains("RULE-SET,nonproxy-mihomo-primary,DIRECT")
    );

    let apply_replay = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id.clone(),
            expected_candidate_hash: prepared.candidate_hash.clone(),
            context: Some(context("apply-replay", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash.clone(),
        }))
        .await
        .unwrap_or_else(|error| panic!("变更应用重放 RPC 失败: {error}"))
        .into_inner();
    assert!(apply_replay.applied);
    assert!(apply_replay.reloaded);
    assert!(apply_replay.replayed);
    assert!(!apply_replay.rolled_back);

    let verified = service
        .verify_change(Request::new(VerifyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id.clone(),
            context: Some(context("verify-1", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("变更验证 RPC 失败: {error}"))
        .into_inner();
    assert!(verified.configuration_verified);
    assert!(!verified.path_verified);
    assert!(!verified.verified);
    assert_eq!(verified.evidence_level, EvidenceLevel::Configuration as i32);

    let rolled_back = service
        .rollback_change(Request::new(RollbackChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id,
            backup_id: prepared.backup_id,
            context: Some(context("rollback-1", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("变更回滚 RPC 失败: {error}"))
        .into_inner();
    assert!(rolled_back.restored);
    assert!(rolled_back.reloaded);
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("已回滚主配置读取失败: {error}")),
        fixture.initial_configuration()
    );

    drop(service);
    let restarted = fixture.service();
    let listed = restarted
        .list_installations(Request::new(ListInstallationsRequest {
            context: Some(context("list-after-restart", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("重启后目录读取失败: {error}"))
        .into_inner();
    assert_eq!(listed.installations.len(), 1);
}

#[tokio::test]
async fn unconfirmed_reload_restores_both_files_and_reloads_the_backup() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let registered = service
        .register_installation(Request::new(
            fixture.registration("register-reload-recovery"),
        ))
        .await
        .unwrap_or_else(|error| panic!("重载恢复测试安装项登记失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    let policy = fixture.policy();
    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-reload-recovery", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("重载恢复测试准备失败: {error}"))
        .into_inner();
    assert!(prepared.error.is_none());
    fixture.controller.set_wrong_rules(true);

    let applied = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id,
            expected_candidate_hash: prepared.candidate_hash,
            context: Some(context("apply-reload-recovery", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash,
        }))
        .await
        .unwrap_or_else(|error| panic!("重载恢复测试应用失败: {error}"))
        .into_inner();

    assert!(!applied.applied);
    assert!(!applied.reloaded);
    assert!(applied.rolled_back);
    assert!(applied.rollback_reloaded);
    assert_eq!(
        applied.error.as_ref().map(|error| error.code.as_str()),
        Some("NP_ADAPTER_CLIENT_RELOAD_UNCONFIRMED")
    );
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("重载恢复后主配置读取失败: {error}")),
        fixture.initial_configuration()
    );
}

#[tokio::test]
async fn failed_reload_replay_preserves_the_previously_applied_candidate() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let registered = service
        .register_installation(Request::new(fixture.registration("register-reload-replay")))
        .await
        .unwrap_or_else(|error| panic!("重载重放测试安装项登记失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    let policy = fixture.policy();
    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-reload-replay", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("重载重放测试准备失败: {error}"))
        .into_inner();
    let first = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id.clone(),
            expected_candidate_hash: prepared.candidate_hash.clone(),
            context: Some(context("apply-before-reload-replay", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash.clone(),
        }))
        .await
        .unwrap_or_else(|error| panic!("重载重放测试首次应用失败: {error}"))
        .into_inner();
    assert!(
        first.applied,
        "首次应用未成功：{:?}",
        first
            .error
            .as_ref()
            .map(|error| (&error.code, &error.message))
    );
    assert!(first.reloaded);
    fixture.controller.set_reload_failure(true);

    let replay = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id,
            expected_candidate_hash: prepared.candidate_hash,
            context: Some(context("apply-failed-reload-replay", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash,
        }))
        .await
        .unwrap_or_else(|error| panic!("重载失败重放 RPC 失败: {error}"))
        .into_inner();

    assert!(
        replay.applied,
        "重放应用未保持已应用状态：{:?}",
        replay
            .error
            .as_ref()
            .map(|error| (&error.code, &error.message))
    );
    assert!(!replay.reloaded);
    assert!(replay.replayed);
    assert!(!replay.rolled_back);
    assert!(!replay.rollback_reloaded);
    assert_eq!(
        replay.error.as_ref().map(|error| error.code.as_str()),
        Some("NP_ADAPTER_CLIENT_RELOAD_FAILED")
    );
    assert!(fixture.managed_path.is_file());
    assert!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("重载失败重放后主配置读取失败: {error}"))
            .contains("RULE-SET,nonproxy-mihomo-primary,DIRECT")
    );
}

#[tokio::test]
async fn external_main_configuration_edit_blocks_both_target_writes() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let registered = service
        .register_installation(Request::new(fixture.registration("register-external")))
        .await
        .unwrap_or_else(|error| panic!("外部修改测试安装项登记失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    let policy = fixture.policy();
    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-external", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("外部修改测试准备失败: {error}"))
        .into_inner();
    assert!(prepared.error.is_none());
    fs::write(&fixture.configuration_path, "external: true\n")
        .unwrap_or_else(|error| panic!("外部主配置模拟失败: {error}"));

    let applied = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id,
            expected_candidate_hash: prepared.candidate_hash,
            context: Some(context("apply-external", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash,
        }))
        .await
        .unwrap_or_else(|error| panic!("外部修改测试应用 RPC 失败: {error}"))
        .into_inner();

    assert!(!applied.applied);
    assert_eq!(
        applied.error.as_ref().map(|error| error.code.as_str()),
        Some("NP_ADAPTER_MANAGED_FILE_CHANGED")
    );
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("受保护主配置读取失败: {error}")),
        "external: true\n"
    );
}

#[tokio::test]
async fn client_upgrade_between_prepare_and_apply_fails_closed() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let registered = service
        .register_installation(Request::new(fixture.registration("register-version")))
        .await
        .unwrap_or_else(|error| panic!("版本门禁安装项登记失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    let policy = fixture.policy();
    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-version", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("版本门禁变更准备失败: {error}"))
        .into_inner();
    assert!(prepared.error.is_none());
    fixture.set_version("1.20.0");

    let applied = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id,
            expected_candidate_hash: prepared.candidate_hash,
            context: Some(context("apply-version", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash,
        }))
        .await
        .unwrap_or_else(|error| panic!("版本门禁应用 RPC 失败: {error}"))
        .into_inner();

    assert!(!applied.applied);
    assert_eq!(
        applied.error.as_ref().map(|error| error.code.as_str()),
        Some("NP_ADAPTER_CLIENT_VERSION_CHANGED")
    );
    assert!(!fixture.managed_path.exists());
}

#[tokio::test]
async fn installation_re_registration_invalidates_a_prepared_change() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let registered = service
        .register_installation(Request::new(fixture.registration("register-binding")))
        .await
        .unwrap_or_else(|error| panic!("绑定测试安装项登记失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    let policy = fixture.policy();
    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-binding", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("绑定测试准备失败: {error}"))
        .into_inner();
    assert!(prepared.error.is_none());
    let removed = service
        .remove_installation(Request::new(RemoveInstallationRequest {
            context: Some(context("remove-binding", TOKEN)),
            adapter_id: "mihomo-primary".to_owned(),
        }))
        .await
        .unwrap_or_else(|error| panic!("绑定测试移除失败: {error}"))
        .into_inner();
    assert!(removed.removed);
    let mut replacement = fixture.registration("register-replacement");
    replacement.direct_target = "DIRECT".to_owned();
    let replaced = service
        .register_installation(Request::new(replacement))
        .await
        .unwrap_or_else(|error| panic!("绑定测试重新登记失败: {error}"))
        .into_inner();
    assert!(replaced.error.is_none());

    let applied = service
        .apply_change(Request::new(ApplyChangeRequest {
            operation_id: String::new(),
            change_id: prepared.change_id,
            expected_candidate_hash: prepared.candidate_hash,
            context: Some(context("apply-binding", TOKEN)),
            expected_configuration_candidate_hash: prepared.configuration_candidate_hash,
        }))
        .await
        .unwrap_or_else(|error| panic!("绑定测试应用失败: {error}"))
        .into_inner();

    assert!(!applied.applied);
    assert_eq!(
        applied.error.as_ref().map(|error| error.code.as_str()),
        Some("NP_ADAPTER_INSTALLATION_CHANGED")
    );
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read_to_string(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("绑定测试主配置读取失败: {error}")),
        fixture.initial_configuration()
    );
}

#[tokio::test]
async fn client_native_validation_failure_creates_no_prepared_change() {
    let fixture = Fixture::new();
    let service = fixture.service();
    let registered = service
        .register_installation(Request::new(fixture.registration("register-validation")))
        .await
        .unwrap_or_else(|error| panic!("校验门禁安装项登记失败: {error}"))
        .into_inner();
    assert!(registered.error.is_none());
    fixture.set_client("1.19.16", false);
    let policy = fixture.policy();

    let prepared = service
        .prepare_change(Request::new(PrepareChangeRequest {
            operation_id: String::new(),
            adapter_id: "mihomo-primary".to_owned(),
            installation_id: "mihomo-primary".to_owned(),
            normalized_policy_hash: Sha256::digest(policy).to_vec(),
            normalized_policy: policy.to_vec(),
            context: Some(context("prepare-invalid-native", TOKEN)),
        }))
        .await
        .unwrap_or_else(|error| panic!("原生校验失败响应异常: {error}"))
        .into_inner();

    assert!(prepared.change_id.is_empty());
    assert!(!prepared.client_validated);
    assert_eq!(
        prepared.error.as_ref().map(|error| error.code.as_str()),
        Some("NP_ADAPTER_CANDIDATE_VALIDATION_FAILED")
    );
    let change_directory = fixture.state.join("transactions/changes");
    let change_count = fs::read_dir(change_directory)
        .unwrap_or_else(|error| panic!("事务目录读取失败: {error}"))
        .count();
    assert_eq!(change_count, 0);
}

#[tokio::test]
async fn private_uds_serves_authenticated_adapter_contract_and_cleans_up() {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};

    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
    let state = root.path().join("runtime");
    let socket = state.join("adapter-host.sock");
    let config = AdapterHostConfig::new(&state, &socket)
        .unwrap_or_else(|error| panic!("适配器宿主配置失败: {error}"));
    let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(nonproxy_adapter_host::run_with_shutdown(config, async {
        let _closed = shutdown_receiver.await;
    }));
    wait_for_path(&socket).await;
    let capability_path = state.join("adapter.capability");
    wait_for_path(&capability_path).await;
    let token = fs::read(&capability_path)
        .unwrap_or_else(|error| panic!("适配器能力文件读取失败: {error}"));
    let metadata = fs::symlink_metadata(&socket)
        .unwrap_or_else(|error| panic!("适配器套接字元数据读取失败: {error}"));
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

    let channel = Endpoint::try_from("http://[::]:50051")
        .unwrap_or_else(|error| panic!("测试 Endpoint 创建失败: {error}"))
        .connect_with_connector(service_fn({
            let socket = socket.clone();
            move |_| {
                let socket = socket.clone();
                async move { UnixStream::connect(socket).await.map(TokioIo::new) }
            }
        }))
        .await
        .unwrap_or_else(|error| panic!("适配器 UDS 连接失败: {error}"));
    let mut client = AdapterServiceClient::new(channel);
    let response = client
        .list_installations(ListInstallationsRequest {
            context: Some(AdapterRequestContext {
                operation_id: "uds-list".to_owned(),
                session_capability_token: token,
            }),
        })
        .await
        .unwrap_or_else(|error| panic!("适配器 UDS RPC 失败: {error}"))
        .into_inner();
    assert!(response.error.is_none());
    assert!(response.installations.is_empty());

    let _sent = shutdown_sender.send(());
    server
        .await
        .unwrap_or_else(|error| panic!("适配器宿主任务 join 失败: {error}"))
        .unwrap_or_else(|error| panic!("适配器宿主退出失败: {error}"));
    assert!(!socket.exists());
}

struct Fixture {
    _root: TempDir,
    controller: MockMihomoController,
    state: std::path::PathBuf,
    executable: std::path::PathBuf,
    managed_path: std::path::PathBuf,
    configuration_path: std::path::PathBuf,
    initial_configuration: String,
}

impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let controller = MockMihomoController::start();
        let state = root.path().join("state");
        let managed = root.path().join("managed");
        fs::create_dir(&state).unwrap_or_else(|error| panic!("状态目录创建失败: {error}"));
        fs::set_permissions(&state, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("状态目录权限设置失败: {error}"));
        fs::create_dir(&managed).unwrap_or_else(|error| panic!("托管目录创建失败: {error}"));
        let executable = root.path().join("mihomo-fixture");
        fs::write(&executable, mihomo_script("1.19.16", true))
            .unwrap_or_else(|error| panic!("Mihomo fixture 写入失败: {error}"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("Mihomo fixture 权限设置失败: {error}"));
        let configuration_path = managed.join("config.yaml");
        let initial_configuration = format!(
            "external-controller: {}\nsecret: nonproxy-test-secret\nmode: rule\nproxies: []\nproxy-groups: []\nrules:\n  - MATCH,DIRECT\n",
            controller.address()
        );
        fs::write(&configuration_path, &initial_configuration)
            .unwrap_or_else(|error| panic!("Mihomo 主配置写入失败: {error}"));
        Self {
            managed_path: managed.join("nonproxy.yaml"),
            configuration_path,
            controller,
            _root: root,
            initial_configuration,
            state,
            executable,
        }
    }

    fn service(&self) -> AdapterRpcService {
        AdapterRpcService::open(
            self.state.join("installations.json"),
            self.state.join("transactions"),
            SessionCapability::from_token(TOKEN),
        )
        .unwrap_or_else(|error| panic!("适配器 RPC 服务创建失败: {error}"))
    }

    fn registration(&self, operation_id: &str) -> RegisterInstallationRequest {
        RegisterInstallationRequest {
            context: Some(context(operation_id, TOKEN)),
            adapter_id: "mihomo-primary".to_owned(),
            client: AdapterClient::Mihomo as i32,
            executable_path: self.executable.to_string_lossy().into_owned(),
            managed_rules_path: self.managed_path.to_string_lossy().into_owned(),
            main_configuration_path: self.configuration_path.to_string_lossy().into_owned(),
            direct_target: String::new(),
        }
    }

    fn initial_configuration(&self) -> &str {
        &self.initial_configuration
    }

    fn policy(&self) -> &'static [u8] {
        br#"{
          "format_version":2,
          "revision":1,
          "rules":[{"id":"site","action":"direct","selector":{
            "kind":"domain","match_kind":"suffix","value":"example.com"
          }}]
        }"#
    }

    fn set_version(&self, version: &str) {
        self.set_client(version, true);
    }

    fn set_client(&self, version: &str, validation_succeeds: bool) {
        fs::write(
            &self.executable,
            mihomo_script(version, validation_succeeds),
        )
        .unwrap_or_else(|error| panic!("Mihomo fixture 版本更新失败: {error}"));
    }
}

fn mihomo_script(version: &str, validation_succeeds: bool) -> String {
    let validation_exit = if validation_succeeds { 0 } else { 1 };
    format!(
        "#!/bin/sh\nif [ \"$1\" = \"-v\" ]; then printf '%s\\n' 'Mihomo Meta v{version} darwin arm64'; exit 0; fi\nif [ \"$1\" = \"-t\" ] && [ \"$2\" = \"-d\" ] && [ \"$4\" = \"-f\" ] && grep -q 'RULE-SET,nonproxy-mihomo-primary,DIRECT' \"$5\" && grep -q 'DOMAIN-SUFFIX,example.com' \"$3/nonproxy.yaml\"; then exit {validation_exit}; fi\nexit 1\n"
    )
}

fn context(operation_id: &str, token: [u8; 32]) -> AdapterRequestContext {
    AdapterRequestContext {
        operation_id: operation_id.to_owned(),
        session_capability_token: token.to_vec(),
    }
}

async fn wait_for_path(path: &Path) {
    for _attempt in 0..100 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("等待适配器运行时文件超时");
}
