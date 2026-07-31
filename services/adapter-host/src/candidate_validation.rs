use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use nonproxy_adapter_api::AdapterClient;
use tempfile::TempDir;

use crate::{
    AdapterHostError,
    detection::DetectedClient,
    path_validation::canonical_executable,
    process_runner::{ProcessExecutionError, ProcessRequest, run},
};

const VALIDATION_TIMEOUT: Duration = Duration::from_secs(5);
const MAXIMUM_COMPILED_RULE_SET_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) async fn validate(
    detected: &DetectedClient,
    candidate: &[u8],
) -> Result<(), AdapterHostError> {
    let client = detected.client;
    let executable = detected.executable_path.clone();
    let candidate = candidate.to_vec();
    let workspace =
        tokio::task::spawn_blocking(move || ValidationWorkspace::create(client, &candidate))
            .await
            .map_err(AdapterHostError::CandidateValidationTask)??;
    let request = workspace.request(client, &executable)?;
    run(request).await.map_err(validation_error)?;
    if client == AdapterClient::SingBox {
        let output = workspace.output_path()?;
        tokio::task::spawn_blocking(move || validate_compiled_output(&output))
            .await
            .map_err(AdapterHostError::CandidateValidationTask)??;
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
    fn create(client: AdapterClient, candidate: &[u8]) -> Result<Self, AdapterHostError> {
        let directory = tempfile::tempdir().map_err(AdapterHostError::CandidateValidationIo)?;
        #[cfg(unix)]
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .map_err(AdapterHostError::CandidateValidationIo)?;
        let (candidate_name, config) = match client {
            AdapterClient::Surge => (
                "nonproxy.rules",
                Some((
                    "nonproxy.conf",
                    b"[General]\n\n[Rule]\nRULE-SET,nonproxy.rules,DIRECT\nFINAL,DIRECT\n".as_slice(),
                )),
            ),
            AdapterClient::Mihomo => (
                "nonproxy.yaml",
                Some((
                    "config.yaml",
                    b"mode: rule\nlog-level: silent\nrule-providers:\n  nonproxy:\n    type: file\n    behavior: classical\n    format: yaml\n    path: ./nonproxy.yaml\nrules:\n  - RULE-SET,nonproxy,DIRECT\n  - MATCH,DIRECT\n".as_slice(),
                )),
            ),
            AdapterClient::SingBox => ("nonproxy.json", None),
        };
        let candidate_path = directory.path().join(candidate_name);
        write_private(&candidate_path, candidate)?;
        let config_path = if let Some((name, bytes)) = config {
            let path = directory.path().join(name);
            write_private(&path, bytes)?;
            Some(path)
        } else {
            None
        };
        let output_path =
            (client == AdapterClient::SingBox).then(|| directory.path().join("nonproxy.srs"));
        Ok(Self {
            directory,
            candidate_path,
            config_path,
            output_path,
        })
    }

    fn request(
        &self,
        client: AdapterClient,
        detected_executable: &Path,
    ) -> Result<ProcessRequest, AdapterHostError> {
        let executable = match client {
            AdapterClient::Surge => surge_cli(detected_executable)?,
            AdapterClient::Mihomo | AdapterClient::SingBox => detected_executable.to_path_buf(),
        };
        let arguments = match client {
            AdapterClient::Surge => vec![
                OsString::from("-c"),
                self.required_config_path()?.as_os_str().to_owned(),
            ],
            AdapterClient::Mihomo => vec![
                OsString::from("-t"),
                OsString::from("-d"),
                self.directory.path().as_os_str().to_owned(),
                OsString::from("-f"),
                self.required_config_path()?.as_os_str().to_owned(),
            ],
            AdapterClient::SingBox => vec![
                OsString::from("rule-set"),
                OsString::from("compile"),
                OsString::from("--output"),
                self.required_output_path()?.as_os_str().to_owned(),
                self.candidate_path.as_os_str().to_owned(),
            ],
        };
        Ok(ProcessRequest {
            executable,
            arguments,
            working_directory: Some(self.directory.path().to_path_buf()),
            home_directory: Some(self.directory.path().to_path_buf()),
            timeout: VALIDATION_TIMEOUT,
        })
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

fn surge_cli(executable: &Path) -> Result<PathBuf, AdapterHostError> {
    let contents = executable
        .parent()
        .and_then(Path::parent)
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("Contents"))
        .ok_or(AdapterHostError::InstallationInvalid)?;
    canonical_executable(&contents.join("Applications/surge-cli"))
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

#[cfg(test)]
mod tests {
    use std::fs;

    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

    use crate::detection::DetectedClient;

    use super::validate;

    #[cfg(unix)]
    #[tokio::test]
    async fn sing_box_validation_requires_a_compiled_output() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let executable = directory.path().join("sing-box");
        fs::write(
            &executable,
            b"#!/bin/sh\nif [ \"$1\" = version ]; then echo 'sing-box version 1.12.4'; exit 0; fi\nif [ \"$1\" = rule-set ] && [ \"$2\" = compile ] && [ \"$3\" = --output ]; then printf 'compiled' > \"$4\"; exit 0; fi\nexit 1\n",
        )
        .unwrap_or_else(|error| panic!("测试客户端写入失败: {error}"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("测试客户端权限设置失败: {error}"));
        let detected = DetectedClient {
            client: AdapterClient::SingBox,
            version: AdapterVersion::new(1, 12, 4),
            executable_path: executable,
        };

        assert!(
            validate(&detected, br#"{"version":3,"rules":[]}"#)
                .await
                .is_ok()
        );
    }

    #[cfg(unix)]
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
            b"#!/bin/sh\nif [ \"$1\" = -c ] && grep -q 'RULE-SET,nonproxy.rules,DIRECT' \"$2\" && grep -q 'DOMAIN-SUFFIX,example.com' nonproxy.rules; then exit 0; fi\nexit 1\n",
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

        assert!(
            validate(&detected, b"DOMAIN-SUFFIX,example.com\n")
                .await
                .is_ok()
        );
    }
}
