use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use nonproxy_adapter_integration::IntegrationPlan;
use nonproxy_adapter_transaction::ChangeInstallation;
use sha2::{Digest, Sha256};

use crate::{
    AdapterHostError,
    client_paths::surge_cli,
    detection::DetectedClient,
    process_runner::{ProcessRequest, run, run_bounded},
};

use super::file::{MAXIMUM_CONFIGURATION_BYTES, sha256_bounded_regular};

const RELOAD_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct SurgeReloadPlan {
    cli: PathBuf,
    integration: IntegrationPlan,
    main_configuration_path: PathBuf,
    backup_sha256: [u8; 32],
    expected_configuration_sha256: Option<[u8; 32]>,
}

impl SurgeReloadPlan {
    pub(crate) fn new(
        detected: &DetectedClient,
        change: &ChangeInstallation,
        main_configuration_path: &Path,
        expected_configuration_sha256: Option<&[u8]>,
    ) -> Result<Self, AdapterHostError> {
        let cli = surge_cli(&detected.executable_path)
            .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        let integration = IntegrationPlan::new(
            detected.client,
            change.adapter_id.clone(),
            main_configuration_path,
            &change.managed_rules_path,
            change.requested_direct_target.clone(),
        )
        .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        let backup_sha256 = change
            .configuration_backup_sha256
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        let expected_configuration_sha256 = expected_configuration_sha256
            .map(|value| {
                value
                    .try_into()
                    .map_err(|_| AdapterHostError::ClientControlUnavailable)
            })
            .transpose()?;
        let current_sha256 = sha256_bounded_regular(main_configuration_path)
            .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        let candidate_sha256 = change
            .configuration_candidate_sha256
            .ok_or(AdapterHostError::ClientControlUnavailable)?;
        if expected_configuration_sha256.is_some_and(|expected| expected != candidate_sha256) {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        let current_is_owned =
            current_sha256 == backup_sha256 || current_sha256 == candidate_sha256;
        if !current_is_owned {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        Ok(Self {
            cli,
            integration,
            main_configuration_path: main_configuration_path.to_path_buf(),
            backup_sha256,
            expected_configuration_sha256,
        })
    }

    pub(crate) async fn preflight(&self) -> Result<(), AdapterHostError> {
        let profile = self
            .read_active_profile()
            .await
            .map_err(|_| AdapterHostError::ClientControlUnavailable)?;
        let actual: [u8; 32] = Sha256::digest(&profile).into();
        let expected_candidate = self.expected_configuration_sha256;
        if actual != self.backup_sha256 && expected_candidate != Some(actual) {
            return Err(AdapterHostError::ClientControlUnavailable);
        }
        Ok(())
    }

    pub(crate) async fn reload(&self, confirm_applied: bool) -> Result<(), AdapterHostError> {
        let expected = if confirm_applied {
            self.expected_configuration_sha256
                .ok_or(AdapterHostError::ClientReloadUnconfirmed)?
        } else {
            self.backup_sha256
        };
        self.verify_configuration(expected)?;
        run(self.request(["reload"]))
            .await
            .map_err(|_| AdapterHostError::ClientReloadFailed)?;
        self.verify_configuration(expected)?;
        let profile = self.read_active_profile().await?;
        let actual: [u8; 32] = Sha256::digest(&profile).into();
        if actual != expected {
            return Err(AdapterHostError::ClientReloadUnconfirmed);
        }
        if !confirm_applied {
            return Ok(());
        }
        let inspection = self
            .integration
            .inspect(&profile)
            .map_err(|_| AdapterHostError::ClientReloadUnconfirmed)?;
        if !inspection.integrated {
            return Err(AdapterHostError::ClientReloadUnconfirmed);
        }
        Ok(())
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

    async fn read_active_profile(&self) -> Result<Vec<u8>, AdapterHostError> {
        run_bounded(
            self.request(["dump", "profile", "original"]),
            MAXIMUM_CONFIGURATION_BYTES,
        )
        .await
        .map_err(|_| AdapterHostError::ClientReloadUnconfirmed)
    }

    fn request<const N: usize>(&self, arguments: [&str; N]) -> ProcessRequest {
        ProcessRequest {
            executable: self.cli.clone(),
            arguments: arguments.into_iter().map(OsString::from).collect(),
            working_directory: self.cli.parent().map(Path::to_path_buf),
            home_directory: None,
            timeout: RELOAD_TIMEOUT,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
    use nonproxy_adapter_transaction::ChangeInstallation;
    use sha2::{Digest, Sha256};

    use crate::{AdapterHostError, detection::DetectedClient};

    use super::SurgeReloadPlan;

    #[tokio::test]
    async fn reload_requires_the_exact_active_integrated_profile() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("Surge 重载测试目录创建失败: {error}"));
        let contents = directory.path().join("Surge.app/Contents");
        let macos = contents.join("MacOS");
        let applications = contents.join("Applications");
        fs::create_dir_all(&macos)
            .and_then(|()| fs::create_dir_all(&applications))
            .unwrap_or_else(|error| panic!("Surge 重载测试目录初始化失败: {error}"));
        let executable = macos.join("Surge");
        let cli = applications.join("surge-cli");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n")
            .and_then(|()| {
                fs::write(
                    &cli,
                    b"#!/bin/sh\nif [ \"$1\" = \"reload\" ]; then exit 0; fi\nif [ \"$1\" = \"dump\" ] && [ \"$2\" = \"profile\" ] && [ \"$3\" = \"original\" ]; then /bin/cat active.conf; exit $?; fi\nexit 1\n",
                )
            })
            .unwrap_or_else(|error| panic!("Surge 重载测试命令写入失败: {error}"));
        for path in [&executable, &cli] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .unwrap_or_else(|error| panic!("Surge 重载测试命令权限设置失败: {error}"));
        }
        let configuration = b"[Rule]\n# >>> NonProxy managed route: surge-primary\nRULE-SET,./nonproxy.list,DIRECT\n# <<< NonProxy managed route: surge-primary\nFINAL,Proxy\n";
        let active_profile = applications.join("active.conf");
        fs::write(&active_profile, configuration)
            .unwrap_or_else(|error| panic!("Surge 活动配置写入失败: {error}"));
        let main_configuration = applications.join("main.conf");
        fs::write(&main_configuration, configuration)
            .unwrap_or_else(|error| panic!("Surge 主配置写入失败: {error}"));
        let configuration_sha256: [u8; 32] = Sha256::digest(configuration).into();
        let change = ChangeInstallation {
            backup_id: "backup".to_owned(),
            adapter_id: "surge-primary".to_owned(),
            client: AdapterClient::Surge,
            client_version: AdapterVersion::new(6, 0, 0),
            managed_rules_path: applications.join("nonproxy.list"),
            main_configuration_path: Some(main_configuration.clone()),
            configuration_backup_sha256: Some(configuration_sha256),
            configuration_candidate_sha256: Some(configuration_sha256),
            direct_target: Some("DIRECT".to_owned()),
            requested_direct_target: None,
        };
        let detected = DetectedClient {
            client: AdapterClient::Surge,
            version: AdapterVersion::new(6, 0, 0),
            executable_path: executable
                .canonicalize()
                .unwrap_or_else(|error| panic!("Surge 测试可执行路径规范化失败: {error}")),
        };
        let plan = SurgeReloadPlan::new(
            &detected,
            &change,
            &main_configuration,
            Some(&configuration_sha256),
        )
        .unwrap_or_else(|error| panic!("Surge 重载计划创建失败: {error}"));

        assert!(plan.preflight().await.is_ok());
        assert!(plan.reload(true).await.is_ok());

        fs::write(&active_profile, b"[Rule]\nFINAL,Proxy\n")
            .unwrap_or_else(|error| panic!("Surge 错误活动配置写入失败: {error}"));
        assert!(matches!(
            plan.reload(true).await,
            Err(AdapterHostError::ClientReloadUnconfirmed)
        ));
    }
}
