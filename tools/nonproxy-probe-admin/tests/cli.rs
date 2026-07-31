use std::{fs, process::Command};

#[test]
fn keygen_and_inspect_cli_keep_the_secret_private_and_refuse_overwrite() {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("CLI 测试目录创建失败: {error}"));
    let key_path = directory.path().join("signing-key.bin");
    let generated = run(&[
        "keygen",
        "--output",
        key_path
            .to_str()
            .unwrap_or_else(|| panic!("CLI 测试密钥路径不是 UTF-8")),
    ]);
    assert!(
        generated.status.success(),
        "keygen 失败: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let metadata = fs::metadata(&key_path)
        .unwrap_or_else(|error| panic!("CLI 测试密钥 metadata 失败: {error}"));
    assert_eq!(metadata.len(), 32);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    let generated_output = String::from_utf8(generated.stdout)
        .unwrap_or_else(|error| panic!("keygen 输出不是 UTF-8: {error}"));
    let generated_lines = generated_output.lines().collect::<Vec<_>>();
    assert_eq!(generated_lines.len(), 3);
    assert!(generated_lines[0].starts_with("key_id="));
    assert_eq!(
        generated_lines[1].strip_prefix("public_key=").map(str::len),
        Some(43)
    );
    assert_eq!(
        generated_lines[2],
        format!("secret_file={}", key_path.display())
    );

    let inspected = run(&[
        "inspect",
        "--input",
        key_path
            .to_str()
            .unwrap_or_else(|| panic!("CLI 测试密钥路径不是 UTF-8")),
    ]);
    assert!(
        inspected.status.success(),
        "inspect 失败: {}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected_output = String::from_utf8(inspected.stdout)
        .unwrap_or_else(|error| panic!("inspect 输出不是 UTF-8: {error}"));
    assert_eq!(
        inspected_output.lines().collect::<Vec<_>>(),
        generated_lines[..2]
    );

    let duplicate = run(&[
        "keygen",
        "--output",
        key_path
            .to_str()
            .unwrap_or_else(|| panic!("CLI 测试密钥路径不是 UTF-8")),
    ]);
    assert!(!duplicate.status.success());
    let unchanged = fs::metadata(&key_path)
        .unwrap_or_else(|error| panic!("重复 keygen 后密钥 metadata 失败: {error}"));
    assert_eq!(unchanged.len(), 32);
}

fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_nonproxy-probe-admin"))
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("CLI 测试进程启动失败: {error}"))
}
