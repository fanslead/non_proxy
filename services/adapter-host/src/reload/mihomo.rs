use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use ::http::{Method, StatusCode};
use nonproxy_adapter_transaction::ChangeInstallation;
use serde::Deserialize;
use serde_yaml_ng::{Mapping, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::AdapterHostError;

use super::{file::read_bounded_regular, http};

const CONTROL_KEYS: [&str; 5] = [
    "external-controller",
    "external-controller-tls",
    "external-controller-unix",
    "external-controller-pipe",
    "secret",
];

pub(crate) struct MihomoReloadPlan {
    endpoint: SocketAddr,
    secret: Zeroizing<String>,
    reload_body: Vec<u8>,
    configuration_path: PathBuf,
    backup_sha256: [u8; 32],
    expected_configuration_sha256: Option<[u8; 32]>,
    reference_name: String,
    direct_target: String,
}

impl MihomoReloadPlan {
    pub(crate) fn new(
        change: &ChangeInstallation,
        main_configuration_path: &Path,
        expected_configuration_sha256: Option<&[u8]>,
    ) -> Result<Self, AdapterHostError> {
        let expected_configuration_sha256 = expected_configuration_sha256
            .map(|value| {
                value
                    .try_into()
                    .map_err(|_| AdapterHostError::ClientControlUnavailable)
            })
            .transpose()?;
        let bytes = Zeroizing::new(read_bounded_regular(main_configuration_path)?);
        let expected_backup = change
            .configuration_backup_sha256
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let actual_configuration: [u8; 32] = Sha256::digest(&bytes).into();
        let expected_candidate = change
            .configuration_candidate_sha256
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        if expected_configuration_sha256.is_some_and(|expected| expected != expected_candidate) {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        let current_is_owned =
            actual_configuration == expected_backup || actual_configuration == expected_candidate;
        if !current_is_owned {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        validate_unique_control_keys(text)?;
        let mut root = parse_root(text)?;
        reject_alternate_controllers(&root)?;
        let endpoint = string_value(&root, "external-controller")
            .ok_or(AdapterHostError::ClientControlUnavailable)
            .and_then(parse_loopback_endpoint)?;
        let secret = match root.remove(Value::String("secret".to_owned())) {
            Some(Value::String(value)) => Zeroizing::new(value),
            None | Some(Value::Null) => Zeroizing::new(String::new()),
            Some(_) => return Err(AdapterHostError::ClientControlUnavailable),
        };
        let direct_target = change
            .direct_target
            .clone()
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let configuration_path = main_configuration_path
            .to_str()
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let reload_body = serde_json::to_vec(&serde_json::json!({
            "path": configuration_path,
            "payload": "",
        }))
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        Ok(Self {
            endpoint,
            secret,
            reload_body,
            configuration_path: main_configuration_path.to_path_buf(),
            backup_sha256: expected_backup,
            expected_configuration_sha256,
            reference_name: format!("nonproxy-{}", change.adapter_id),
            direct_target,
        })
    }

    pub(crate) async fn preflight(&self) -> Result<(), AdapterHostError> {
        let (status, _body) =
            http::request(self.endpoint, Method::GET, "/version", &self.secret, &[])
                .await
                .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        if status == StatusCode::OK {
            Ok(())
        } else {
            Err(AdapterHostError::ClientControlUnavailable)
        }
    }

    pub(crate) async fn reload(&self, confirm_applied: bool) -> Result<(), AdapterHostError> {
        let expected = if confirm_applied {
            self.expected_configuration_sha256
                .ok_or(AdapterHostError::ClientReloadUnconfirmed)?
        } else {
            self.backup_sha256
        };
        self.verify_configuration(expected)?;
        let (status, _body) = http::request(
            self.endpoint,
            Method::PUT,
            "/configs?force=true",
            &self.secret,
            &self.reload_body,
        )
        .await?;
        if status != StatusCode::NO_CONTENT {
            return Err(AdapterHostError::ClientReloadFailed);
        }
        self.verify_configuration(expected)?;
        if !confirm_applied {
            return Ok(());
        }
        self.confirm_rules().await
    }

    fn verify_configuration(&self, expected: [u8; 32]) -> Result<(), AdapterHostError> {
        let bytes = Zeroizing::new(
            read_bounded_regular(&self.configuration_path)
                .map_err(|_| AdapterHostError::ClientReloadUnconfirmed)?,
        );
        let actual: [u8; 32] = Sha256::digest(&bytes).into();
        if actual == expected {
            Ok(())
        } else {
            Err(AdapterHostError::ClientReloadUnconfirmed)
        }
    }

    async fn confirm_rules(&self) -> Result<(), AdapterHostError> {
        let (status, body) =
            http::request(self.endpoint, Method::GET, "/rules", &self.secret, &[]).await?;
        if status != StatusCode::OK {
            return Err(AdapterHostError::ClientReloadUnconfirmed);
        }
        let response: RulesResponse =
            serde_json::from_slice(&body).map_err(|_| AdapterHostError::ClientReloadUnconfirmed)?;
        let first = response
            .rules
            .first()
            .ok_or(AdapterHostError::ClientReloadUnconfirmed)?;
        if first.payload != self.reference_name || first.proxy != self.direct_target {
            return Err(AdapterHostError::ClientReloadUnconfirmed);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct RulesResponse {
    rules: Vec<LoadedRule>,
}

#[derive(Deserialize)]
struct LoadedRule {
    payload: String,
    proxy: String,
}

fn parse_root(input: &str) -> Result<Mapping, AdapterHostError> {
    match serde_yaml_ng::from_str::<Value>(input)
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?
    {
        Value::Mapping(root) => Ok(root),
        _ => Err(AdapterHostError::ClientControlUnavailable),
    }
}

fn validate_unique_control_keys(input: &str) -> Result<(), AdapterHostError> {
    if CONTROL_KEYS
        .iter()
        .any(|key| semantic_top_level_key_count(input, key) > 1)
    {
        return Err(AdapterHostError::ClientControlUnavailable);
    }
    Ok(())
}

fn reject_alternate_controllers(root: &Mapping) -> Result<(), AdapterHostError> {
    for key in [
        "external-controller-tls",
        "external-controller-unix",
        "external-controller-pipe",
    ] {
        if mapping_value(root, key).is_some_and(|value| {
            !matches!(value, Value::Null)
                && !matches!(value, Value::String(text) if text.is_empty())
        }) {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
    }
    Ok(())
}

fn parse_loopback_endpoint(value: &str) -> Result<SocketAddr, AdapterHostError> {
    let endpoint =
        SocketAddr::from_str(value).map_err(|_| AdapterHostError::ClientControlUnavailable)?;
    if endpoint.port() == 0 || !endpoint.ip().is_loopback() {
        return Err(AdapterHostError::ClientControlUnavailable);
    }
    Ok(endpoint)
}

fn semantic_top_level_key_count(input: &str, key: &str) -> usize {
    input
        .lines()
        .filter(|line| {
            if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
                return false;
            }
            let Some((candidate, _value)) = line.split_once(':') else {
                return false;
            };
            serde_yaml_ng::from_str::<Value>(candidate.trim())
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .is_some_and(|value| value == key)
        })
        .count()
}

fn mapping_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_owned()))
}

fn string_value<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a str> {
    mapping_value(mapping, key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
    use nonproxy_adapter_transaction::ChangeInstallation;
    use sha2::{Digest, Sha256};

    use super::MihomoReloadPlan;

    fn change(path: PathBuf) -> ChangeInstallation {
        let configuration = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("Mihomo 控制测试配置读取失败: {error}"));
        ChangeInstallation {
            backup_id: "backup".to_owned(),
            adapter_id: "mihomo-primary".to_owned(),
            client: AdapterClient::Mihomo,
            client_version: AdapterVersion::new(1, 19, 28),
            managed_rules_path: path.with_file_name("nonproxy.yaml"),
            main_configuration_path: Some(path),
            configuration_backup_sha256: Some(Sha256::digest(configuration).into()),
            configuration_candidate_sha256: Some([1; 32]),
            direct_target: Some("DIRECT".to_owned()),
            requested_direct_target: None,
        }
    }

    #[test]
    fn accepts_only_one_plain_loopback_controller() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("Mihomo 控制测试目录创建失败: {error}"));
        let path = directory.path().join("config.yaml");
        std::fs::write(
            &path,
            "external-controller: 127.0.0.1:9090\nsecret: local-only\n",
        )
        .unwrap_or_else(|error| panic!("Mihomo 控制测试配置写入失败: {error}"));
        assert!(MihomoReloadPlan::new(&change(path.clone()), &path, Some(&[1; 32])).is_ok());

        std::fs::write(
            &path,
            "external-controller: 0.0.0.0:9090\nsecret: local-only\n",
        )
        .unwrap_or_else(|error| panic!("Mihomo 非回环配置写入失败: {error}"));
        assert!(MihomoReloadPlan::new(&change(path.clone()), &path, Some(&[1; 32])).is_err());

        std::fs::write(
            &path,
            "external-controller: 127.0.0.1:9090\n'external-controller': 127.0.0.1:9091\n",
        )
        .unwrap_or_else(|error| panic!("Mihomo 重复控制配置写入失败: {error}"));
        assert!(MihomoReloadPlan::new(&change(path.clone()), &path, Some(&[1; 32])).is_err());

        std::fs::write(
            &path,
            "external-controller: 127.0.0.1:9090\nexternal-controller-tls: 127.0.0.1:9443\n",
        )
        .unwrap_or_else(|error| panic!("Mihomo TLS 控制配置写入失败: {error}"));
        assert!(MihomoReloadPlan::new(&change(path.clone()), &path, Some(&[1; 32])).is_err());
    }
}
