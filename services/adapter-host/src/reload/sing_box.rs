use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    time::Duration,
};

use sysinfo::{Pid, Signal, System};

use nonproxy_adapter_transaction::ChangeInstallation;

use crate::{AdapterHostError, detection::DetectedClient};

use super::file::sha256_bounded_regular;

const CONFIRMATION_INTERVAL: Duration = Duration::from_millis(75);
const CONFIRMATION_ATTEMPTS: usize = 10;

pub(crate) struct SingBoxReloadPlan {
    pid: Pid,
    start_time: u64,
    executable_path: PathBuf,
    main_configuration_path: PathBuf,
    backup_sha256: [u8; 32],
    expected_configuration_sha256: Option<[u8; 32]>,
}

impl SingBoxReloadPlan {
    pub(crate) fn new(
        detected: &DetectedClient,
        change: &ChangeInstallation,
        main_configuration_path: &Path,
        expected_configuration_sha256: Option<&[u8]>,
    ) -> Result<Self, AdapterHostError> {
        let backup_sha256 = change
            .configuration_backup_sha256
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let candidate_sha256 = change
            .configuration_candidate_sha256
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let expected_configuration_sha256 = expected_configuration_sha256
            .map(|value| {
                value
                    .try_into()
                    .map_err(|_| AdapterHostError::ClientControlUnavailable)
            })
            .transpose()?;
        if expected_configuration_sha256.is_some_and(|expected| expected != candidate_sha256) {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        let current_sha256 = sha256_bounded_regular(main_configuration_path)
            .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        let current_is_owned =
            current_sha256 == backup_sha256 || current_sha256 == candidate_sha256;
        if !current_is_owned {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        let system = System::new_all();
        let current_pid =
            sysinfo::get_current_pid().map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        let current_user = system
            .process(current_pid)
            .and_then(sysinfo::Process::effective_user_id)
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let mut matches = system.processes().values().filter(|process| {
            process.effective_user_id() == Some(current_user)
                && process_matches(process, &detected.executable_path, main_configuration_path)
        });
        let process = matches
            .next()
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        if matches.next().is_some() {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        Ok(Self {
            pid: process.pid(),
            start_time: process.start_time(),
            executable_path: detected.executable_path.clone(),
            main_configuration_path: main_configuration_path.to_path_buf(),
            backup_sha256,
            expected_configuration_sha256,
        })
    }

    pub(crate) async fn reload(&self, confirm_applied: bool) -> Result<(), AdapterHostError> {
        let expected = if confirm_applied {
            self.expected_configuration_sha256
                .ok_or(AdapterHostError::ClientReloadUnconfirmed)?
        } else {
            self.backup_sha256
        };
        self.verify_configuration(expected)?;
        let system = System::new_all();
        let process = system
            .process(self.pid)
            .filter(|process| {
                process.start_time() == self.start_time
                    && process_matches(
                        process,
                        &self.executable_path,
                        &self.main_configuration_path,
                    )
            })
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        if process.kill_with(Signal::Hangup) != Some(true) {
            return Err(AdapterHostError::ClientReloadFailed);
        }
        for _attempt in 0..CONFIRMATION_ATTEMPTS {
            tokio::time::sleep(CONFIRMATION_INTERVAL).await;
            let refreshed = System::new_all();
            let alive = refreshed.process(self.pid).is_some_and(|process| {
                process.start_time() == self.start_time
                    && process_matches(
                        process,
                        &self.executable_path,
                        &self.main_configuration_path,
                    )
            });
            if !alive {
                return Err(AdapterHostError::ClientReloadFailed);
            }
        }
        self.verify_configuration(expected)
    }

    pub(crate) fn preflight(&self) -> Result<(), AdapterHostError> {
        let system = System::new_all();
        if system.process(self.pid).is_some_and(|process| {
            process.start_time() == self.start_time
                && process_matches(
                    process,
                    &self.executable_path,
                    &self.main_configuration_path,
                )
        }) {
            Ok(())
        } else {
            Err(AdapterHostError::ClientControlUnavailable)
        }
    }

    fn verify_configuration(&self, expected: [u8; 32]) -> Result<(), AdapterHostError> {
        if sha256_bounded_regular(&self.main_configuration_path)
            .is_ok_and(|actual| actual == expected)
        {
            Ok(())
        } else {
            Err(AdapterHostError::ClientReloadUnconfirmed)
        }
    }
}

fn process_matches(
    process: &sysinfo::Process,
    executable_path: &Path,
    main_configuration_path: &Path,
) -> bool {
    let executable_matches = process
        .exe()
        .and_then(|path| path.canonicalize().ok())
        .is_some_and(|path| path == executable_path);
    executable_matches
        && command_uses_only_configuration(process.cmd(), process.cwd(), main_configuration_path)
}

fn command_uses_only_configuration(
    command: &[OsString],
    working_directory: Option<&Path>,
    expected: &Path,
) -> bool {
    let mut configurations = Vec::new();
    let mut run_subcommand = false;
    let mut index = 1_usize;
    while index < command.len() {
        let argument = &command[index];
        if matches!(argument.to_str(), Some("-C" | "--config-directory"))
            || argument.to_str().is_some_and(|value| {
                value.starts_with("--config-directory=") || value.starts_with("-C=")
            })
        {
            return false;
        }
        if matches!(argument.to_str(), Some("-c" | "--config")) {
            let Some(value) = command.get(index + 1) else {
                return false;
            };
            configurations.push(value.as_os_str());
            index += 2;
            continue;
        }
        if argument == "run" {
            if run_subcommand {
                return false;
            }
            run_subcommand = true;
        } else if matches!(
            argument.to_str(),
            Some("check" | "format" | "generate" | "merge" | "rule-set" | "tools" | "version")
        ) {
            return false;
        }
        if let Some(value) = argument.to_str().and_then(|value| {
            value
                .strip_prefix("--config=")
                .or_else(|| value.strip_prefix("-c="))
        }) {
            configurations.push(OsStr::new(value));
        }
        index += 1;
    }
    if !run_subcommand || configurations.len() != 1 {
        return false;
    }
    resolve_process_path(configurations[0], working_directory).is_some_and(|path| path == expected)
}

fn resolve_process_path(value: &OsStr, working_directory: Option<&Path>) -> Option<PathBuf> {
    let path = Path::new(value);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory?.join(path)
    };
    path.canonicalize().ok()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::command_uses_only_configuration;

    #[test]
    fn command_binding_requires_one_exact_config_file() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("sing-box 命令测试目录创建失败: {error}"));
        let config = directory.path().join("config.json");
        std::fs::write(&config, "{}")
            .unwrap_or_else(|error| panic!("sing-box 命令测试配置写入失败: {error}"));
        let config = config
            .canonicalize()
            .unwrap_or_else(|error| panic!("sing-box 命令测试配置规范化失败: {error}"));
        let exact = ["sing-box", "run", "-c", "config.json"]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        assert!(command_uses_only_configuration(
            &exact,
            Some(directory.path()),
            &config
        ));

        let merged = ["sing-box", "-c", "config.json", "-c", "other.json", "run"]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        assert!(!command_uses_only_configuration(
            &merged,
            Some(directory.path()),
            &config
        ));

        let directory_mode = ["sing-box", "run", "-C", "."]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        assert!(!command_uses_only_configuration(
            &directory_mode,
            Some(directory.path()),
            &config
        ));

        let named_run = directory.path().join("run");
        std::fs::write(&named_run, "{}")
            .unwrap_or_else(|error| panic!("sing-box 伪子命令配置写入失败: {error}"));
        let named_run = named_run
            .canonicalize()
            .unwrap_or_else(|error| panic!("sing-box 伪子命令配置规范化失败: {error}"));
        let check_with_run_config = ["sing-box", "check", "-c", "run"]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>();
        assert!(!command_uses_only_configuration(
            &check_with_run_config,
            Some(directory.path()),
            &named_run
        ));
    }
}
