use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use nonproxy_adapter_api::AdapterClient;
use nonproxy_adapter_transaction::IntegratedCandidate;
use tempfile::TempDir;

use crate::{
    AdapterHostError,
    client_paths::surge_cli,
    detection::DetectedClient,
    process_runner::{ProcessExecutionError, ProcessRequest, run},
};

const VALIDATION_TIMEOUT: Duration = Duration::from_secs(5);
const MAXIMUM_COMPILED_RULE_SET_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) async fn validate_integrated(
    detected: &DetectedClient,
    main_configuration_path: &Path,
    candidate: &IntegratedCandidate,
) -> Result<(), AdapterHostError> {
    let client = detected.client;
    let executable = detected.executable_path.clone();
    let configuration_name = main_configuration_path
        .file_name()
        .ok_or(AdapterHostError::InstallationInvalid)?
        .to_owned();
    let rules = candidate.rendered_rules().bytes().to_vec();
    let configuration = candidate.configuration_bytes().to_vec();
    let managed_reference = candidate.managed_rules_reference().to_owned();
    let workspace = tokio::task::spawn_blocking(move || {
        ValidationWorkspace::create(
            client,
            &configuration_name,
            &managed_reference,
            &rules,
            &configuration,
        )
    })
    .await
    .map_err(AdapterHostError::CandidateValidationTask)??;
    let requests = workspace.requests(client, &executable)?;
    for (index, request) in requests.into_iter().enumerate() {
        run(request).await.map_err(validation_error)?;
        if client == AdapterClient::SingBox && index == 0 {
            let output = workspace.output_path()?;
            tokio::task::spawn_blocking(move || validate_compiled_output(&output))
                .await
                .map_err(AdapterHostError::CandidateValidationTask)??;
        }
    }
    Ok(())
}

struct ValidationWorkspace {
    directory: TempDir,
    candidate_path: PathBuf,
    config_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
}

impl ValidationWorkspace {
    fn create(
        client: AdapterClient,
        configuration_name: &OsStr,
        managed_reference: &str,
        rules: &[u8],
        configuration: &[u8],
    ) -> Result<Self, AdapterHostError> {
        let directory = tempfile::tempdir().map_err(AdapterHostError::CandidateValidationIo)?;
        #[cfg(unix)]
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .map_err(AdapterHostError::CandidateValidationIo)?;
        let managed_path = managed_reference_path(managed_reference)?;
        let candidate_path = directory.path().join(managed_path);
        if let Some(parent) = candidate_path.parent() {
            create_private_directories(directory.path(), parent)?;
        }
        write_private(&candidate_path, rules)?;
        let config_path = directory.path().join(configuration_name);
        if config_path == candidate_path {
            return Err(AdapterHostError::InstallationInvalid);
        }
        write_private(&config_path, configuration)?;
        let output_path =
            (client == AdapterClient::SingBox).then(|| directory.path().join("nonproxy.srs"));
        Ok(Self {
            directory,
            candidate_path,
            config_path: Some(config_path),
            output_path,
        })
    }

    fn requests(
        &self,
        client: AdapterClient,
        detected_executable: &Path,
    ) -> Result<Vec<ProcessRequest>, AdapterHostError> {
        let executable = match client {
            AdapterClient::Surge => surge_cli(detected_executable)?,
            AdapterClient::Mihomo | AdapterClient::SingBox => detected_executable.to_path_buf(),
        };
        let arguments = match client {
            AdapterClient::Surge => vec![vec![
                OsString::from("-c"),
                self.required_config_path()?.as_os_str().to_owned(),
            ]],
            AdapterClient::Mihomo => vec![vec![
                OsString::from("-t"),
                OsString::from("-d"),
                self.directory.path().as_os_str().to_owned(),
                OsString::from("-f"),
                self.required_config_path()?.as_os_str().to_owned(),
            ]],
            AdapterClient::SingBox => vec![
                vec![
                    OsString::from("rule-set"),
                    OsString::from("compile"),
                    OsString::from("--output"),
                    self.required_output_path()?.as_os_str().to_owned(),
                    self.candidate_path.as_os_str().to_owned(),
                ],
                vec![
                    OsString::from("check"),
                    OsString::from("-c"),
                    self.required_config_path()?.as_os_str().to_owned(),
                ],
            ],
        };
        Ok(arguments
            .into_iter()
            .map(|arguments| ProcessRequest {
                executable: executable.clone(),
                arguments,
                working_directory: Some(self.directory.path().to_path_buf()),
                home_directory: Some(self.directory.path().to_path_buf()),
                timeout: VALIDATION_TIMEOUT,
            })
            .collect())
    }

    fn required_config_path(&self) -> Result<&Path, AdapterHostError> {
        self.config_path
            .as_deref()
            .ok_or(AdapterHostError::CandidateValidationFailed)
    }

    fn required_output_path(&self) -> Result<&Path, AdapterHostError> {
        self.output_path
            .as_deref()
            .ok_or(AdapterHostError::CandidateValidationFailed)
    }

    fn output_path(&self) -> Result<PathBuf, AdapterHostError> {
        self.output_path
            .clone()
            .ok_or(AdapterHostError::CandidateValidationFailed)
    }
}

fn managed_reference_path(value: &str) -> Result<&Path, AdapterHostError> {
    let relative = value
        .strip_prefix("./")
        .ok_or(AdapterHostError::InstallationInvalid)?;
    let path = Path::new(relative);
    if path.file_name().is_none()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(path)
}

fn create_private_directories(root: &Path, target: &Path) -> Result<(), AdapterHostError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| AdapterHostError::InstallationInvalid)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(AdapterHostError::InstallationInvalid);
        };
        current.push(value);
        fs::create_dir(&current)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(AdapterHostError::CandidateValidationIo)?;
        #[cfg(unix)]
        fs::set_permissions(&current, fs::Permissions::from_mode(0o700))
            .map_err(AdapterHostError::CandidateValidationIo)?;
    }
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), AdapterHostError> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(AdapterHostError::CandidateValidationIo)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(AdapterHostError::CandidateValidationIo)
}

fn validate_compiled_output(path: &Path) -> Result<(), AdapterHostError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options
        .open(path)
        .map_err(AdapterHostError::CandidateValidationIo)?;
    let metadata = file
        .metadata()
        .map_err(AdapterHostError::CandidateValidationIo)?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAXIMUM_COMPILED_RULE_SET_BYTES
    {
        return Err(AdapterHostError::CandidateValidationFailed);
    }
    Ok(())
}

fn validation_error(error: ProcessExecutionError) -> AdapterHostError {
    match error {
        ProcessExecutionError::Io(source) => AdapterHostError::CandidateValidationIo(source),
        ProcessExecutionError::Task(source) => AdapterHostError::CandidateValidationTask(source),
        ProcessExecutionError::Failed => AdapterHostError::CandidateValidationFailed,
        ProcessExecutionError::Timeout | ProcessExecutionError::OutputTooLarge => {
            AdapterHostError::CandidateValidationUnavailable
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;

    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

    use crate::detection::DetectedClient;

    use nonproxy_adapter_transaction::{AdapterInstallation, AdapterTransactionManager};

    use super::validate_integrated;

    const POLICY: &[u8] = br#"{
      "format_version":2,
      "revision":1,
      "rules":[{"id":"site","action":"direct","selector":{
        "kind":"domain","match_kind":"suffix","value":"example.com"
      }}]
    }"#;

    #[tokio::test]
    async fn sing_box_validation_requires_a_compiled_output() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let executable = directory.path().join("sing-box");
        fs::write(
            &executable,
            b"#!/bin/sh\nif [ \"$1\" = version ]; then echo 'sing-box version 1.12.4'; exit 0; fi\nif [ \"$1\" = rule-set ] && [ \"$2\" = compile ] && [ \"$3\" = --output ]; then printf 'compiled' > \"$4\"; exit 0; fi\nif [ \"$1\" = check ] && [ \"$2\" = -c ] && grep -q 'nonproxy-sing' \"$3\" && grep -q 'example.com' nonproxy.json; then printf checked > \"$0.checked\"; exit 0; fi\nexit 1\n",
        )
        .unwrap_or_else(|error| panic!("测试客户端写入失败: {error}"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("测试客户端权限设置失败: {error}"));
        let detected = DetectedClient {
            client: AdapterClient::SingBox,
            version: AdapterVersion::new(1, 12, 4),
            executable_path: executable,
        };
        let configuration_path = directory.path().join("config.json");
        let managed_path = directory.path().join("nonproxy.json");
        fs::write(
            &configuration_path,
            br#"{"outbounds":[{"type":"direct","tag":"direct"}]}"#,
        )
        .unwrap_or_else(|error| panic!("sing-box 主配置写入失败: {error}"));
        let installation = AdapterInstallation::new(
            "sing",
            AdapterClient::SingBox,
            AdapterVersion::new(1, 12, 4),
            managed_path,
        );
        let candidate = AdapterTransactionManager::preview_integrated(
            &installation,
            &configuration_path,
            None,
            POLICY,
        )
        .unwrap_or_else(|error| panic!("sing-box 候选生成失败: {error}"));

        assert!(
            validate_integrated(&detected, &configuration_path, &candidate)
                .await
                .is_ok()
        );
        assert!(directory.path().join("sing-box.checked").is_file());
    }

    #[tokio::test]
    async fn surge_validation_uses_only_the_selected_bundle_cli_and_candidate() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let contents = directory.path().join("Surge.app/Contents");
        let macos = contents.join("MacOS");
        let applications = contents.join("Applications");
        fs::create_dir_all(&macos)
            .and_then(|()| fs::create_dir_all(&applications))
            .unwrap_or_else(|error| panic!("Surge 测试包创建失败: {error}"));
        let executable = macos.join("Surge");
        let cli = applications.join("surge-cli");
        fs::write(&executable, b"fixture")
            .unwrap_or_else(|error| panic!("Surge 测试入口写入失败: {error}"));
        fs::write(
            &cli,
            b"#!/bin/sh\nif [ \"$1\" = -c ] && grep -q 'RULE-SET,./nonproxy.rules,DIRECT' \"$2\" && grep -q 'DOMAIN-SUFFIX,example.com' nonproxy.rules; then exit 0; fi\nexit 1\n",
        )
        .unwrap_or_else(|error| panic!("Surge CLI fixture 写入失败: {error}"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .and_then(|()| fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)))
            .unwrap_or_else(|error| panic!("Surge 测试入口权限设置失败: {error}"));
        let detected = DetectedClient {
            client: AdapterClient::Surge,
            version: AdapterVersion::new(6, 1, 2),
            executable_path: executable,
        };
        let configuration_path = directory.path().join("surge.conf");
        let managed_path = directory.path().join("nonproxy.rules");
        fs::write(&configuration_path, b"[General]\n\n[Rule]\nFINAL,DIRECT\n")
            .unwrap_or_else(|error| panic!("Surge 主配置写入失败: {error}"));
        let installation = AdapterInstallation::new(
            "surge",
            AdapterClient::Surge,
            AdapterVersion::new(6, 1, 2),
            managed_path,
        );
        let candidate = AdapterTransactionManager::preview_integrated(
            &installation,
            &configuration_path,
            None,
            POLICY,
        )
        .unwrap_or_else(|error| panic!("Surge 候选生成失败: {error}"));

        assert!(
            validate_integrated(&detected, &configuration_path, &candidate)
                .await
                .is_ok()
        );
    }
}
