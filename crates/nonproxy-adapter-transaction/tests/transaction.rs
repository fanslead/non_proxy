use std::{fs, path::Path};

use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
use nonproxy_adapter_transaction::{
    AdapterInstallation, AdapterTransactionError, AdapterTransactionManager,
};
use tempfile::TempDir;

const NOW: u64 = 10_000;

#[test]
fn new_managed_file_applies_verifies_and_rolls_back_to_absence() {
    let fixture = Fixture::new("new-file");
    let prepared = fixture.prepare(NOW);

    let applied = fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1)
        .unwrap_or_else(|error| panic!("候选应用失败: {error}"));
    let verified = fixture
        .manager
        .verify(&prepared.change_id)
        .unwrap_or_else(|error| panic!("候选验证失败: {error}"));
    let rolled_back = fixture
        .manager
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("候选回滚失败: {error}"));

    assert!(applied.applied);
    assert!(!applied.replayed);
    assert!(verified.configuration_verified);
    assert!(!verified.path_verified);
    assert!(rolled_back.restored);
    assert!(!rolled_back.replayed);
    assert!(!fixture.managed_path.exists());
}

#[test]
fn existing_file_and_process_restart_keep_idempotent_recovery() {
    let fixture = Fixture::new("existing-file");
    let original = b"payload:\n  - 'DOMAIN,old.example'\n";
    fs::write(&fixture.managed_path, original)
        .unwrap_or_else(|error| panic!("旧规则写入失败: {error}"));
    let prepared = fixture.prepare(NOW);
    fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1)
        .unwrap_or_else(|error| panic!("候选应用失败: {error}"));
    let replay = fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 2)
        .unwrap_or_else(|error| panic!("应用重放失败: {error}"));

    let restarted = AdapterTransactionManager::open(fixture.state_path())
        .unwrap_or_else(|error| panic!("事务管理器重启失败: {error}"));
    let restored = restarted
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("重启后回滚失败: {error}"));
    let rollback_replay = restarted
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("回滚重放失败: {error}"));

    assert!(replay.replayed);
    assert!(!restored.replayed);
    assert!(rollback_replay.replayed);
    assert_eq!(
        fs::read(&fixture.managed_path).unwrap_or_else(|error| panic!("恢复内容读取失败: {error}")),
        original
    );
}

#[test]
fn external_changes_are_never_overwritten() {
    let fixture = Fixture::new("external-before");
    fs::write(&fixture.managed_path, b"old")
        .unwrap_or_else(|error| panic!("旧规则写入失败: {error}"));
    let prepared = fixture.prepare(NOW);
    fs::write(&fixture.managed_path, b"external")
        .unwrap_or_else(|error| panic!("外部规则写入失败: {error}"));

    let apply = fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1);

    assert!(matches!(
        apply,
        Err(AdapterTransactionError::ManagedFileChanged)
    ));
    assert_eq!(
        fs::read(&fixture.managed_path).unwrap_or_else(|error| panic!("外部内容读取失败: {error}")),
        b"external"
    );
}

#[test]
fn rollback_refuses_to_replace_a_new_external_edit() {
    let fixture = Fixture::new("external-after");
    fs::write(&fixture.managed_path, b"old")
        .unwrap_or_else(|error| panic!("旧规则写入失败: {error}"));
    let prepared = fixture.prepare(NOW);
    fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1)
        .unwrap_or_else(|error| panic!("候选应用失败: {error}"));
    fs::write(&fixture.managed_path, b"external")
        .unwrap_or_else(|error| panic!("外部规则写入失败: {error}"));

    let rollback = fixture
        .manager
        .rollback(&prepared.change_id, &prepared.backup_id);

    assert!(matches!(
        rollback,
        Err(AdapterTransactionError::ManagedFileChanged)
    ));
    assert_eq!(
        fs::read(&fixture.managed_path).unwrap_or_else(|error| panic!("外部内容读取失败: {error}")),
        b"external"
    );
}

#[test]
fn expired_and_tampered_candidates_fail_closed() {
    let fixture = Fixture::new("expiry");
    let prepared = fixture.prepare(NOW);
    let expired = fixture.manager.apply(
        &prepared.change_id,
        &prepared.candidate_sha256,
        prepared.expires_at_unix_ms + 1,
    );
    assert!(matches!(
        expired,
        Err(AdapterTransactionError::ChangeExpired)
    ));

    let candidate = fixture
        .state_path()
        .join("candidates")
        .join(format!("{}.rules", prepared.change_id));
    fs::write(candidate, b"tampered").unwrap_or_else(|error| panic!("候选篡改失败: {error}"));
    let tampered = fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1);
    assert!(matches!(
        tampered,
        Err(AdapterTransactionError::StateCorrupt)
    ));
}

#[cfg(unix)]
#[test]
fn managed_symlink_is_rejected_without_touching_its_target() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("symlink");
    let outside = fixture.root.path().join("outside.rules");
    fs::write(&outside, b"outside").unwrap_or_else(|error| panic!("外部规则写入失败: {error}"));
    symlink(&outside, &fixture.managed_path)
        .unwrap_or_else(|error| panic!("测试符号链接创建失败: {error}"));

    let result = fixture.manager.prepare(
        &fixture.installation,
        &fixture.operation_id,
        fixture.policy_bytes(),
        NOW,
    );

    assert!(matches!(
        result,
        Err(AdapterTransactionError::ManagedPathInvalid)
    ));
    assert_eq!(
        fs::read(outside).unwrap_or_else(|error| panic!("外部规则读取失败: {error}")),
        b"outside"
    );
}

#[cfg(unix)]
#[test]
fn state_and_transaction_files_are_owner_only() {
    let fixture = Fixture::new("permissions");
    let prepared = fixture.prepare(NOW);
    let state = fixture.state_path();
    let manifest = state
        .join("changes")
        .join(format!("{}.json", prepared.change_id));
    let candidate = state
        .join("candidates")
        .join(format!("{}.rules", prepared.change_id));

    assert_eq!(mode(state), 0o700);
    assert_eq!(mode(&manifest), 0o600);
    assert_eq!(mode(&candidate), 0o600);
}

#[test]
fn prepare_operation_is_idempotent_but_rejects_changed_replay() {
    let fixture = Fixture::new("idempotent");
    let first = fixture.prepare(NOW);
    let replay = fixture.prepare(NOW + 50);

    assert_eq!(replay, first);
    fixture
        .manager
        .apply(&first.change_id, &first.candidate_sha256, NOW + 60)
        .unwrap_or_else(|error| panic!("幂等测试候选应用失败: {error}"));
    assert_eq!(fixture.prepare(NOW + 70), first);
    let changed = br#"{
      "format_version":1,
      "revision":2,
      "rules":[{"id":"other","action":"direct","selector":{
        "kind":"domain","match_kind":"exact","value":"changed.example"
      }}]
    }"#;
    let conflict = fixture.manager.prepare(
        &fixture.installation,
        &fixture.operation_id,
        changed,
        NOW + 100,
    );
    assert!(matches!(
        conflict,
        Err(AdapterTransactionError::ChangeConflict)
    ));
}

#[test]
fn expired_cleanup_preserves_applied_or_externally_changed_recovery_state() {
    let fixture = Fixture::new("preserve-recovery");
    fs::write(&fixture.managed_path, b"old")
        .unwrap_or_else(|error| panic!("旧规则写入失败: {error}"));
    let prepared = fixture.prepare(NOW);
    fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1)
        .unwrap_or_else(|error| panic!("候选应用失败: {error}"));
    fs::write(&fixture.managed_path, b"external")
        .unwrap_or_else(|error| panic!("外部规则写入失败: {error}"));

    let _next = fixture
        .manager
        .prepare(
            &fixture.installation,
            "operation-next",
            fixture.policy_bytes(),
            prepared.expires_at_unix_ms + 1,
        )
        .unwrap_or_else(|error| panic!("下一变更准备失败: {error}"));

    assert!(
        fixture
            .state_path()
            .join("changes")
            .join(format!("{}.json", prepared.change_id))
            .is_file(),
        "外部修改发生后必须保留旧变更和备份供人工恢复"
    );
}

#[test]
fn change_cleanup_requires_the_backup_to_be_restored() {
    let fixture = Fixture::new("safe-cleanup");
    fs::write(&fixture.managed_path, b"old")
        .unwrap_or_else(|error| panic!("旧规则写入失败: {error}"));
    let prepared = fixture.prepare(NOW);
    fixture
        .manager
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1)
        .unwrap_or_else(|error| panic!("候选应用失败: {error}"));

    assert!(matches!(
        fixture.manager.remove_change(&prepared.change_id),
        Err(AdapterTransactionError::ChangeConflict)
    ));
    fixture
        .manager
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("候选回滚失败: {error}"));
    fixture
        .manager
        .remove_change(&prepared.change_id)
        .unwrap_or_else(|error| panic!("已恢复变更清理失败: {error}"));
    assert!(
        !fixture
            .state_path()
            .join("changes")
            .join(format!("{}.json", prepared.change_id))
            .exists()
    );
}

#[test]
fn restart_removes_unreferenced_crash_files_and_rejects_referenced_tampering() {
    let fixture = Fixture::new("recovery-scan");
    let orphan_candidate = fixture.state_path().join("candidates/orphan.rules");
    let orphan_backup = fixture.state_path().join("backups/orphan.rules");
    fs::write(&orphan_candidate, b"orphan")
        .unwrap_or_else(|error| panic!("孤儿候选写入失败: {error}"));
    fs::write(&orphan_backup, b"orphan")
        .unwrap_or_else(|error| panic!("孤儿备份写入失败: {error}"));

    let _restarted = AdapterTransactionManager::open(fixture.state_path())
        .unwrap_or_else(|error| panic!("事务恢复扫描失败: {error}"));
    assert!(!orphan_candidate.exists());
    assert!(!orphan_backup.exists());

    let prepared = fixture.prepare(NOW);
    let candidate = fixture
        .state_path()
        .join("candidates")
        .join(format!("{}.rules", prepared.change_id));
    fs::write(candidate, b"tampered").unwrap_or_else(|error| panic!("候选篡改失败: {error}"));
    assert!(matches!(
        AdapterTransactionManager::open(fixture.state_path()),
        Err(AdapterTransactionError::StateCorrupt)
    ));
}

#[test]
fn version_two_sidecar_manifest_remains_recoverable() {
    let fixture = Fixture::new("manifest-v2");
    let prepared = fixture.prepare(NOW);
    let manifest_path = fixture
        .state_path()
        .join("changes")
        .join(format!("{}.json", prepared.change_id));
    let bytes =
        fs::read(&manifest_path).unwrap_or_else(|error| panic!("manifest 读取失败: {error}"));
    let mut document: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("manifest JSON 解析失败: {error}"));
    document["format_version"] = serde_json::Value::from(2);
    let bytes = serde_json::to_vec(&document)
        .unwrap_or_else(|error| panic!("v2 manifest 编码失败: {error}"));
    fs::write(&manifest_path, bytes)
        .unwrap_or_else(|error| panic!("v2 manifest 写入失败: {error}"));

    let restarted = AdapterTransactionManager::open(fixture.state_path())
        .unwrap_or_else(|error| panic!("v2 manifest 恢复失败: {error}"));
    restarted
        .apply(&prepared.change_id, &prepared.candidate_sha256, NOW + 1)
        .unwrap_or_else(|error| panic!("v2 manifest 应用失败: {error}"));
}

#[test]
fn integrated_change_applies_verifies_and_rolls_back_both_files() {
    let fixture = IntegratedFixture::new("integrated-round-trip");
    let original_configuration = fs::read(&fixture.configuration_path)
        .unwrap_or_else(|error| panic!("主配置读取失败: {error}"));
    let prepared = fixture.prepare(NOW);

    let applied = fixture
        .manager
        .apply_integrated(
            &prepared.change_id,
            &prepared.candidate_sha256,
            &prepared_configuration_hash(&prepared),
            NOW + 1,
        )
        .unwrap_or_else(|error| panic!("双文件应用失败: {error}"));
    let configuration = fs::read_to_string(&fixture.configuration_path)
        .unwrap_or_else(|error| panic!("接入后主配置读取失败: {error}"));
    let verified = fixture
        .manager
        .verify(&prepared.change_id)
        .unwrap_or_else(|error| panic!("双文件验证失败: {error}"));
    assert!(fixture.managed_path.is_file());
    let rolled_back = fixture
        .manager
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("双文件回滚失败: {error}"));

    assert!(applied.applied);
    assert!(!applied.replayed);
    assert!(configuration.contains("# preserve user comment"));
    assert!(configuration.contains("RULE-SET,nonproxy-mihomo-integrated-round-trip,DIRECT"));
    assert!(verified.configuration_verified);
    assert_eq!(
        verified.configuration_candidate_sha256,
        prepared.configuration_candidate_sha256
    );
    assert!(rolled_back.restored);
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("回滚主配置读取失败: {error}")),
        original_configuration
    );
}

#[cfg(unix)]
#[test]
fn integrated_change_preserves_main_configuration_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = IntegratedFixture::new("integrated-mode");
    fs::set_permissions(
        &fixture.configuration_path,
        fs::Permissions::from_mode(0o640),
    )
    .unwrap_or_else(|error| panic!("主配置权限设置失败: {error}"));
    let prepared = fixture.prepare(NOW);
    let configuration_candidate = fixture
        .state_path()
        .join("candidates")
        .join(format!("{}.config", prepared.change_id));
    let configuration_backup = fixture
        .state_path()
        .join("backups")
        .join(format!("{}.config", prepared.backup_id));
    assert_eq!(mode(&configuration_candidate), 0o600);
    assert_eq!(mode(&configuration_backup), 0o600);
    fixture
        .manager
        .apply_integrated(
            &prepared.change_id,
            &prepared.candidate_sha256,
            &prepared_configuration_hash(&prepared),
            NOW + 1,
        )
        .unwrap_or_else(|error| panic!("双文件应用失败: {error}"));
    assert_eq!(mode(&fixture.configuration_path), 0o640);
    fixture
        .manager
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("双文件回滚失败: {error}"));
    assert_eq!(mode(&fixture.configuration_path), 0o640);
}

#[test]
fn integrated_prepare_rejects_a_configuration_changed_after_preview() {
    let fixture = IntegratedFixture::new("integrated-preview-race");
    let preview = AdapterTransactionManager::preview_integrated(
        &fixture.installation,
        &fixture.configuration_path,
        None,
        fixture.policy_bytes(),
    )
    .unwrap_or_else(|error| panic!("双文件候选预览失败: {error}"));
    fs::write(
        &fixture.configuration_path,
        b"mixed-port: 7891\nrules:\n  - MATCH,Proxy\n",
    )
    .unwrap_or_else(|error| panic!("预览后主配置修改失败: {error}"));

    let result = fixture.manager.prepare_integrated(
        &fixture.installation,
        &fixture.configuration_path,
        None,
        &fixture.operation_id,
        fixture.policy_bytes(),
        preview.rendered_rules().sha256(),
        preview.configuration_sha256(),
        NOW,
    );

    assert!(matches!(
        result,
        Err(AdapterTransactionError::CandidateHashMismatch)
    ));
    assert_eq!(
        fs::read_dir(fixture.state_path().join("changes"))
            .unwrap_or_else(|error| panic!("变更目录读取失败: {error}"))
            .count(),
        0
    );
}

#[test]
fn integrated_apply_preflights_external_main_configuration_before_sidecar_write() {
    let fixture = IntegratedFixture::new("integrated-external-before");
    let prepared = fixture.prepare(NOW);
    fs::write(&fixture.configuration_path, b"external: true\n")
        .unwrap_or_else(|error| panic!("外部主配置写入失败: {error}"));

    let result = fixture.manager.apply_integrated(
        &prepared.change_id,
        &prepared.candidate_sha256,
        &prepared_configuration_hash(&prepared),
        NOW + 1,
    );

    assert!(matches!(
        result,
        Err(AdapterTransactionError::ManagedFileChanged)
    ));
    assert!(!fixture.managed_path.exists());
    assert_eq!(
        fs::read(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("外部主配置读取失败: {error}")),
        b"external: true\n"
    );
}

#[test]
fn restart_rolls_back_a_sidecar_only_partial_apply() {
    let fixture = IntegratedFixture::new("integrated-partial-apply");
    let prepared = fixture.prepare(NOW);
    let rules_candidate = fixture
        .state_path()
        .join("candidates")
        .join(format!("{}.rules", prepared.change_id));
    fs::copy(rules_candidate, &fixture.managed_path)
        .unwrap_or_else(|error| panic!("半完成 sidecar 模拟失败: {error}"));

    let restarted = AdapterTransactionManager::open(fixture.state_path())
        .unwrap_or_else(|error| panic!("半完成应用恢复失败: {error}"));

    assert!(!fixture.managed_path.exists());
    let applied = restarted
        .apply_integrated(
            &prepared.change_id,
            &prepared.candidate_sha256,
            &prepared_configuration_hash(&prepared),
            NOW + 1,
        )
        .unwrap_or_else(|error| panic!("恢复后双文件应用失败: {error}"));
    assert!(applied.applied);
}

#[test]
fn restart_completes_a_main_first_partial_rollback() {
    let fixture = IntegratedFixture::new("integrated-partial-rollback");
    let prepared = fixture.prepare(NOW);
    fixture
        .manager
        .apply_integrated(
            &prepared.change_id,
            &prepared.candidate_sha256,
            &prepared_configuration_hash(&prepared),
            NOW + 1,
        )
        .unwrap_or_else(|error| panic!("双文件应用失败: {error}"));
    let configuration_backup = fixture
        .state_path()
        .join("backups")
        .join(format!("{}.config", prepared.backup_id));
    fs::copy(configuration_backup, &fixture.configuration_path)
        .unwrap_or_else(|error| panic!("半完成主配置回滚模拟失败: {error}"));

    let restarted = AdapterTransactionManager::open(fixture.state_path())
        .unwrap_or_else(|error| panic!("半完成回滚恢复失败: {error}"));
    let replay = restarted
        .rollback(&prepared.change_id, &prepared.backup_id)
        .unwrap_or_else(|error| panic!("恢复后回滚重放失败: {error}"));

    assert!(!fixture.managed_path.exists());
    assert!(replay.replayed);
}

#[test]
fn integrated_rollback_never_overwrites_an_external_main_edit() {
    let fixture = IntegratedFixture::new("integrated-external-after");
    let prepared = fixture.prepare(NOW);
    fixture
        .manager
        .apply_integrated(
            &prepared.change_id,
            &prepared.candidate_sha256,
            &prepared_configuration_hash(&prepared),
            NOW + 1,
        )
        .unwrap_or_else(|error| panic!("双文件应用失败: {error}"));
    let sidecar =
        fs::read(&fixture.managed_path).unwrap_or_else(|error| panic!("sidecar 读取失败: {error}"));
    fs::write(&fixture.configuration_path, b"external: true\n")
        .unwrap_or_else(|error| panic!("外部主配置写入失败: {error}"));

    let rollback = fixture
        .manager
        .rollback(&prepared.change_id, &prepared.backup_id);

    assert!(matches!(
        rollback,
        Err(AdapterTransactionError::ManagedFileChanged)
    ));
    assert_eq!(
        fs::read(&fixture.managed_path)
            .unwrap_or_else(|error| panic!("受保护 sidecar 读取失败: {error}")),
        sidecar
    );
    assert_eq!(
        fs::read(&fixture.configuration_path)
            .unwrap_or_else(|error| panic!("外部主配置读取失败: {error}")),
        b"external: true\n"
    );
}

#[test]
fn restart_rejects_a_tampered_integrated_configuration_candidate() {
    let fixture = IntegratedFixture::new("integrated-tampered-candidate");
    let prepared = fixture.prepare(NOW);
    let candidate = fixture
        .state_path()
        .join("candidates")
        .join(format!("{}.config", prepared.change_id));
    fs::write(candidate, b"tampered").unwrap_or_else(|error| panic!("主配置候选篡改失败: {error}"));

    assert!(matches!(
        AdapterTransactionManager::open(fixture.state_path()),
        Err(AdapterTransactionError::StateCorrupt)
    ));
}

#[test]
fn integrated_apply_recovers_owned_atomic_temporary_files() {
    let fixture = IntegratedFixture::new("integrated-atomic-temporary");
    let prepared = fixture.prepare(NOW);
    let rules_temporary = fixture
        .managed_path
        .parent()
        .unwrap_or_else(|| panic!("sidecar 缺少父目录"))
        .join(format!(
            ".nonproxy.yaml.nonproxy-{}.tmp",
            prepared.change_id
        ));
    let configuration_temporary = fixture
        .configuration_path
        .parent()
        .unwrap_or_else(|| panic!("主配置缺少父目录"))
        .join(format!(
            ".config,user.yaml.nonproxy-{}.config.tmp",
            prepared.change_id
        ));
    fs::write(&rules_temporary, b"partial")
        .and_then(|()| fs::write(&configuration_temporary, b"partial"))
        .unwrap_or_else(|error| panic!("崩溃临时文件模拟失败: {error}"));

    fixture
        .manager
        .apply_integrated(
            &prepared.change_id,
            &prepared.candidate_sha256,
            &prepared_configuration_hash(&prepared),
            NOW + 1,
        )
        .unwrap_or_else(|error| panic!("临时文件恢复后应用失败: {error}"));

    assert!(!rules_temporary.exists());
    assert!(!configuration_temporary.exists());
}

struct IntegratedFixture {
    _root: TempDir,
    manager: AdapterTransactionManager,
    installation: AdapterInstallation,
    managed_path: std::path::PathBuf,
    configuration_path: std::path::PathBuf,
    state_path: std::path::PathBuf,
    operation_id: String,
}

impl IntegratedFixture {
    fn new(name: &str) -> Self {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let managed_directory = root.path().join("managed");
        fs::create_dir(&managed_directory)
            .unwrap_or_else(|error| panic!("托管目录创建失败: {error}"));
        let managed_path = managed_directory.join("nonproxy.yaml");
        let configuration_path = managed_directory.join("config,user.yaml");
        fs::write(
            &configuration_path,
            b"# preserve user comment\nmixed-port: 7890\nrules:\n  - MATCH,Proxy\n",
        )
        .unwrap_or_else(|error| panic!("主配置写入失败: {error}"));
        let state_path = root.path().join("state");
        let manager = AdapterTransactionManager::open(&state_path)
            .unwrap_or_else(|error| panic!("事务管理器创建失败: {error}"));
        let installation = AdapterInstallation::new(
            format!("mihomo-{name}"),
            AdapterClient::Mihomo,
            AdapterVersion::new(1, 18, 0),
            managed_path.clone(),
        );
        Self {
            _root: root,
            manager,
            installation,
            managed_path,
            configuration_path,
            state_path,
            operation_id: format!("operation-{name}"),
        }
    }

    fn prepare(&self, now_unix_ms: u64) -> nonproxy_adapter_transaction::PreparedChange {
        let preview = AdapterTransactionManager::preview_integrated(
            &self.installation,
            &self.configuration_path,
            None,
            self.policy_bytes(),
        )
        .unwrap_or_else(|error| panic!("双文件候选预览失败: {error}"));
        self.manager
            .prepare_integrated(
                &self.installation,
                &self.configuration_path,
                None,
                &self.operation_id,
                self.policy_bytes(),
                preview.rendered_rules().sha256(),
                preview.configuration_sha256(),
                now_unix_ms,
            )
            .unwrap_or_else(|error| panic!("双文件候选准备失败: {error}"))
    }

    fn policy_bytes(&self) -> &'static [u8] {
        br#"{
          "format_version":1,
          "revision":1,
          "rules":[{"id":"site","action":"direct","selector":{
            "kind":"domain","match_kind":"suffix","value":"example.com"
          }}]
        }"#
    }

    fn state_path(&self) -> &Path {
        &self.state_path
    }
}

fn prepared_configuration_hash(
    prepared: &nonproxy_adapter_transaction::PreparedChange,
) -> [u8; 32] {
    prepared
        .configuration_candidate_sha256
        .unwrap_or_else(|| panic!("双文件变更缺少主配置哈希"))
}

struct Fixture {
    root: TempDir,
    manager: AdapterTransactionManager,
    installation: AdapterInstallation,
    managed_path: std::path::PathBuf,
    state_path: std::path::PathBuf,
    operation_id: String,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let managed_directory = root.path().join("managed");
        fs::create_dir(&managed_directory)
            .unwrap_or_else(|error| panic!("托管目录创建失败: {error}"));
        let managed_path = managed_directory.join(format!("{name}.yaml"));
        let state_path = root.path().join("state");
        let manager = AdapterTransactionManager::open(&state_path)
            .unwrap_or_else(|error| panic!("事务管理器创建失败: {error}"));
        let installation = AdapterInstallation::new(
            format!("mihomo-{name}"),
            AdapterClient::Mihomo,
            AdapterVersion::new(1, 18, 0),
            managed_path.clone(),
        );
        Self {
            root,
            manager,
            installation,
            managed_path,
            state_path,
            operation_id: format!("operation-{name}"),
        }
    }

    fn prepare(&self, now_unix_ms: u64) -> nonproxy_adapter_transaction::PreparedChange {
        self.manager
            .prepare(
                &self.installation,
                &self.operation_id,
                self.policy_bytes(),
                now_unix_ms,
            )
            .unwrap_or_else(|error| panic!("候选准备失败: {error}"))
    }

    fn policy_bytes(&self) -> &'static [u8] {
        br#"{
          "format_version":1,
          "revision":1,
          "rules":[{"id":"site","action":"direct","selector":{
            "kind":"domain","match_kind":"suffix","value":"example.com"
          }}]
        }"#
    }

    fn state_path(&self) -> &Path {
        &self.state_path
    }
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .unwrap_or_else(|error| panic!("权限读取失败: {error}"))
        .permissions()
        .mode()
        & 0o777
}
