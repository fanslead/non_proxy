use std::{
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    time::Duration,
};

use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time::timeout,
};

use crate::{AdapterHostError, capabilities::capabilities, path_validation::canonical_executable};

const DETECTION_TIMEOUT: Duration = Duration::from_secs(3);
const MAXIMUM_OUTPUT_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectedClient {
    pub client: AdapterClient,
    pub version: AdapterVersion,
    pub executable_path: PathBuf,
}

impl DetectedClient {
    #[must_use]
    pub fn supported(&self) -> bool {
        !capabilities(self.client, self.version).is_empty()
    }
}

pub async fn detect(
    client: AdapterClient,
    executable_path: &Path,
) -> Result<DetectedClient, AdapterHostError> {
    let executable_path = canonical_executable(executable_path)?;
    let output = match client {
        AdapterClient::Surge => {
            let info_plist = surge_info_plist(&executable_path)?;
            run_command(
                Path::new("/usr/bin/plutil"),
                &["-extract", "CFBundleShortVersionString", "raw", "-o", "-"],
                Some(&info_plist),
                DETECTION_TIMEOUT,
            )
            .await?
        }
        AdapterClient::Mihomo => {
            run_command(&executable_path, &["-v"], None, DETECTION_TIMEOUT).await?
        }
        AdapterClient::SingBox => {
            run_command(&executable_path, &["version"], None, DETECTION_TIMEOUT).await?
        }
    };
    let version = parse_version(client, &output)?;
    Ok(DetectedClient {
        client,
        version,
        executable_path,
    })
}

async fn run_command(
    executable: &Path,
    arguments: &[&str],
    final_path_argument: Option<&Path>,
    command_timeout: Duration,
) -> Result<Vec<u8>, AdapterHostError> {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .env_clear()
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = final_path_argument {
        command.arg(path);
    }
    let mut child = command.spawn().map_err(AdapterHostError::DetectionIo)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(AdapterHostError::DetectionFailed)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(AdapterHostError::DetectionFailed)?;
    let stdout_task = tokio::spawn(read_bounded(stdout));
    let stderr_task = tokio::spawn(read_bounded(stderr));
    let status = match timeout(command_timeout, child.wait()).await {
        Ok(result) => result.map_err(AdapterHostError::DetectionIo)?,
        Err(_) => {
            let _kill_result = child.kill().await;
            let _wait_result = child.wait().await;
            let _stdout_result = stdout_task.await;
            let _stderr_result = stderr_task.await;
            return Err(AdapterHostError::DetectionFailed);
        }
    };
    let (stdout, stdout_overflow) = stdout_task
        .await
        .map_err(AdapterHostError::DetectionTask)??;
    let (stderr, stderr_overflow) = stderr_task
        .await
        .map_err(AdapterHostError::DetectionTask)??;
    if !status.success() || stdout_overflow || stderr_overflow {
        return Err(AdapterHostError::DetectionFailed);
    }
    let combined_length = stdout
        .len()
        .checked_add(stderr.len())
        .ok_or(AdapterHostError::DetectionFailed)?;
    if u64::try_from(combined_length).map_or(true, |length| length > MAXIMUM_OUTPUT_BYTES) {
        return Err(AdapterHostError::DetectionFailed);
    }
    let mut output = stdout;
    if !output.is_empty() && !stderr.is_empty() {
        output.push(b'\n');
    }
    output.extend_from_slice(&stderr);
    Ok(output)
}

async fn read_bounded(reader: impl AsyncRead + Unpin) -> Result<(Vec<u8>, bool), AdapterHostError> {
    let mut output = Vec::new();
    reader
        .take(MAXIMUM_OUTPUT_BYTES + 1)
        .read_to_end(&mut output)
        .await
        .map_err(AdapterHostError::DetectionIo)?;
    let overflow = u64::try_from(output.len()).map_or(true, |length| length > MAXIMUM_OUTPUT_BYTES);
    Ok((output, overflow))
}

fn surge_info_plist(executable: &Path) -> Result<PathBuf, AdapterHostError> {
    let macos_directory = executable
        .parent()
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("MacOS"))
        .ok_or(AdapterHostError::InstallationInvalid)?;
    let contents = macos_directory
        .parent()
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("Contents"))
        .ok_or(AdapterHostError::InstallationInvalid)?;
    let bundle = contents
        .parent()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("app"))
        .ok_or(AdapterHostError::InstallationInvalid)?;
    let plist = bundle.join("Contents/Info.plist");
    if !plist.is_file() {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(plist)
}

fn parse_version(client: AdapterClient, output: &[u8]) -> Result<AdapterVersion, AdapterHostError> {
    let value = std::str::from_utf8(output).map_err(|_| AdapterHostError::DetectionFailed)?;
    match client {
        AdapterClient::Surge => parse_version_candidate(value.trim()),
        AdapterClient::Mihomo => {
            let tokens = value.split_ascii_whitespace().collect::<Vec<_>>();
            let start = tokens
                .iter()
                .position(|token| token.to_ascii_lowercase().contains("mihomo"))
                .ok_or(AdapterHostError::DetectionFailed)?;
            tokens
                .iter()
                .skip(start + 1)
                .filter(|token| token.starts_with('v') || token.starts_with('V'))
                .find_map(|token| parse_version_candidate(token).ok())
                .ok_or(AdapterHostError::DetectionFailed)
        }
        AdapterClient::SingBox => {
            let mut tokens = value.split_ascii_whitespace();
            while let Some(token) = tokens.next() {
                if token.eq_ignore_ascii_case("version") {
                    return tokens
                        .next()
                        .ok_or(AdapterHostError::DetectionFailed)
                        .and_then(parse_version_candidate);
                }
            }
            Err(AdapterHostError::DetectionFailed)
        }
    }
}

fn parse_version_candidate(value: &str) -> Result<AdapterVersion, AdapterHostError> {
    let trimmed = value.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric() && !matches!(character, '.' | '-')
    });
    let candidate = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    if candidate.is_empty()
        || !candidate
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
    {
        return Err(AdapterHostError::DetectionFailed);
    }
    AdapterVersion::from_str(candidate).map_err(|_| AdapterHostError::DetectionFailed)
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

    use super::{parse_version, run_command};

    #[test]
    fn parses_only_client_scoped_version_output() {
        assert!(matches!(
            parse_version(AdapterClient::Mihomo, b"Mihomo Meta v1.19.3 darwin arm64"),
            Ok(value) if value == AdapterVersion::new(1, 19, 3)
        ));
        assert!(matches!(
            parse_version(AdapterClient::SingBox, b"sing-box version 1.12.4\nEnvironment"),
            Ok(value) if value == AdapterVersion::new(1, 12, 4)
        ));
        assert!(matches!(
            parse_version(AdapterClient::Surge, b"6.1.2\n"),
            Ok(value) if value == AdapterVersion::new(6, 1, 2)
        ));
        assert!(parse_version(AdapterClient::SingBox, b"library 9.9.9").is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn command_runner_has_timeout_and_output_bound() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let slow = directory.path().join("slow");
        fs::write(&slow, b"#!/bin/sh\nsleep 2\n")
            .unwrap_or_else(|error| panic!("慢命令写入失败: {error}"));
        fs::set_permissions(&slow, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("慢命令权限设置失败: {error}"));

        assert!(
            run_command(&slow, &[], None, Duration::from_millis(20))
                .await
                .is_err()
        );
    }
}
