use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

use crate::{
    AdapterHostError,
    capabilities::capabilities,
    path_validation::canonical_executable,
    process_runner::{ProcessExecutionError, ProcessRequest, run},
};

const DETECTION_TIMEOUT: Duration = Duration::from_secs(3);

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
            run(ProcessRequest {
                executable: PathBuf::from("/usr/bin/plutil"),
                arguments: ["-extract", "CFBundleShortVersionString", "raw", "-o", "-"]
                    .into_iter()
                    .map(OsString::from)
                    .chain([info_plist.into_os_string()])
                    .collect(),
                working_directory: None,
                home_directory: None,
                timeout: DETECTION_TIMEOUT,
            })
            .await
            .map_err(detection_error)?
        }
        AdapterClient::Mihomo => run(ProcessRequest {
            executable: executable_path.clone(),
            arguments: vec![OsString::from("-v")],
            working_directory: None,
            home_directory: None,
            timeout: DETECTION_TIMEOUT,
        })
        .await
        .map_err(detection_error)?,
        AdapterClient::SingBox => run(ProcessRequest {
            executable: executable_path.clone(),
            arguments: vec![OsString::from("version")],
            working_directory: None,
            home_directory: None,
            timeout: DETECTION_TIMEOUT,
        })
        .await
        .map_err(detection_error)?,
    };
    let version = parse_version(client, &output)?;
    Ok(DetectedClient {
        client,
        version,
        executable_path,
    })
}

fn detection_error(error: ProcessExecutionError) -> AdapterHostError {
    match error {
        ProcessExecutionError::Io(source) => AdapterHostError::DetectionIo(source),
        ProcessExecutionError::Task(source) => AdapterHostError::DetectionTask(source),
        ProcessExecutionError::Timeout
        | ProcessExecutionError::Failed
        | ProcessExecutionError::OutputTooLarge => AdapterHostError::DetectionFailed,
    }
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
    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

    use super::parse_version;

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
}
