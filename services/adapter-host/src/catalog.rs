use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use serde::{Deserialize, Serialize};

use crate::{AdapterHostError, model::RegisteredInstallation};

const CATALOG_FORMAT_VERSION: u32 = 2;
const LEGACY_CATALOG_FORMAT_VERSION: u32 = 1;
const MAXIMUM_CATALOG_BYTES: u64 = 1024 * 1024;
const MAXIMUM_INSTALLATIONS: usize = 32;
const TEMPORARY_ATTEMPTS: usize = 4;

pub struct InstallationCatalog {
    path: PathBuf,
    entries: Mutex<BTreeMap<String, RegisteredInstallation>>,
}

impl InstallationCatalog {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AdapterHostError> {
        let path = path.into();
        let entries = read_catalog(&path)?;
        Ok(Self {
            path,
            entries: Mutex::new(entries),
        })
    }

    pub fn register(&self, installation: RegisteredInstallation) -> Result<bool, AdapterHostError> {
        validate_identifier(&installation.adapter_id)?;
        validate_stored_path(&installation.executable_path)?;
        validate_stored_path(&installation.managed_rules_path)?;
        if let Some(path) = installation.main_configuration_path.as_deref() {
            validate_stored_path(path)?;
        }
        validate_direct_target(installation.direct_target.as_deref())?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| AdapterHostError::CatalogCorrupt)?;
        if let Some(existing) = entries.get(&installation.adapter_id) {
            return if existing == &installation {
                Ok(true)
            } else {
                Err(AdapterHostError::InstallationConflict)
            };
        }
        if entries.len() >= MAXIMUM_INSTALLATIONS {
            return Err(AdapterHostError::InstallationLimitReached);
        }
        let mut next = entries.clone();
        next.insert(installation.adapter_id.clone(), installation);
        write_catalog(&self.path, &next)?;
        *entries = next;
        Ok(false)
    }

    pub fn get(&self, adapter_id: &str) -> Result<RegisteredInstallation, AdapterHostError> {
        validate_identifier(adapter_id)?;
        self.entries
            .lock()
            .map_err(|_| AdapterHostError::CatalogCorrupt)?
            .get(adapter_id)
            .cloned()
            .ok_or(AdapterHostError::InstallationNotFound)
    }

    pub fn list(&self) -> Result<Vec<RegisteredInstallation>, AdapterHostError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| AdapterHostError::CatalogCorrupt)?
            .values()
            .cloned()
            .collect())
    }

    pub fn remove(&self, adapter_id: &str) -> Result<bool, AdapterHostError> {
        validate_identifier(adapter_id)?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| AdapterHostError::CatalogCorrupt)?;
        if !entries.contains_key(adapter_id) {
            return Ok(false);
        }
        let mut next = entries.clone();
        next.remove(adapter_id);
        write_catalog(&self.path, &next)?;
        *entries = next;
        Ok(true)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CatalogDocument {
    format_version: u32,
    installations: Vec<RegisteredInstallation>,
}

fn read_catalog(path: &Path) -> Result<BTreeMap<String, RegisteredInstallation>, AdapterHostError> {
    let Some(bytes) = read_optional_private(path)? else {
        return Ok(BTreeMap::new());
    };
    let document: CatalogDocument =
        serde_json::from_slice(&bytes).map_err(|_| AdapterHostError::CatalogCorrupt)?;
    if !matches!(
        document.format_version,
        LEGACY_CATALOG_FORMAT_VERSION | CATALOG_FORMAT_VERSION
    ) || document.installations.len() > MAXIMUM_INSTALLATIONS
    {
        return Err(AdapterHostError::CatalogCorrupt);
    }
    let mut entries = BTreeMap::new();
    for installation in document.installations {
        validate_identifier(&installation.adapter_id)
            .map_err(|_| AdapterHostError::CatalogCorrupt)?;
        validate_stored_path(&installation.executable_path)
            .map_err(|_| AdapterHostError::CatalogCorrupt)?;
        validate_stored_path(&installation.managed_rules_path)
            .map_err(|_| AdapterHostError::CatalogCorrupt)?;
        if let Some(path) = installation.main_configuration_path.as_deref() {
            validate_stored_path(path).map_err(|_| AdapterHostError::CatalogCorrupt)?;
        }
        validate_direct_target(installation.direct_target.as_deref())
            .map_err(|_| AdapterHostError::CatalogCorrupt)?;
        if entries
            .insert(installation.adapter_id.clone(), installation)
            .is_some()
        {
            return Err(AdapterHostError::CatalogCorrupt);
        }
    }
    Ok(entries)
}

fn read_optional_private(path: &Path) -> Result<Option<Vec<u8>>, AdapterHostError> {
    #[cfg(windows)]
    match fs::symlink_metadata(path) {
        Ok(_) => nonproxy_windows_security::validate_regular_file(path)
            .map_err(|_| AdapterHostError::CatalogCorrupt)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AdapterHostError::File(error)),
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AdapterHostError::File(error)),
    };
    let metadata = file.metadata().map_err(AdapterHostError::File)?;
    if !metadata.is_file() || metadata.len() > MAXIMUM_CATALOG_BYTES {
        return Err(AdapterHostError::CatalogCorrupt);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(AdapterHostError::CatalogCorrupt);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len()).map_err(|_| AdapterHostError::CatalogCorrupt)?,
    );
    (&mut file)
        .take(MAXIMUM_CATALOG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(AdapterHostError::File)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAXIMUM_CATALOG_BYTES) {
        return Err(AdapterHostError::CatalogCorrupt);
    }
    Ok(Some(bytes))
}

fn write_catalog(
    path: &Path,
    entries: &BTreeMap<String, RegisteredInstallation>,
) -> Result<(), AdapterHostError> {
    let document = CatalogDocument {
        format_version: CATALOG_FORMAT_VERSION,
        installations: entries.values().cloned().collect(),
    };
    let bytes = serde_json::to_vec(&document).map_err(|_| AdapterHostError::CatalogCorrupt)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAXIMUM_CATALOG_BYTES) {
        return Err(AdapterHostError::CatalogCorrupt);
    }
    let parent = path.parent().ok_or(AdapterHostError::Configuration)?;
    let (temporary, mut file) = create_temporary(parent)?;
    let result = file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(AdapterHostError::File);
    drop(file);
    if let Err(error) = result {
        let _cleanup = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = replace_file(&temporary, path) {
        cleanup_failed_replacement(&temporary);
        return Err(AdapterHostError::File(error));
    }
    sync_directory(parent)
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, File), AdapterHostError> {
    let mut collision = None;
    for _attempt in 0..TEMPORARY_ATTEMPTS {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(AdapterHostError::Random)?;
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = parent.join(format!(".installations.{suffix}.tmp"));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&path) {
            Ok(file) => {
                #[cfg(windows)]
                if let Err(error) = nonproxy_windows_security::protect_current_user_file(&path) {
                    drop(file);
                    let _cleanup = fs::remove_file(&path);
                    return Err(AdapterHostError::File(error));
                }
                return Ok((path, file));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                collision = Some(error);
            }
            Err(error) => return Err(AdapterHostError::File(error)),
        }
    }
    Err(AdapterHostError::File(collision.unwrap_or_else(|| {
        std::io::Error::other("temporary catalog path allocation exhausted")
    })))
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    nonproxy_windows_security::replace_file_atomically(source, target)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn cleanup_failed_replacement(_path: &Path) {}

#[cfg(not(windows))]
fn cleanup_failed_replacement(path: &Path) {
    let _cleanup = fs::remove_file(path);
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), AdapterHostError> {
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<(), AdapterHostError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(AdapterHostError::File)
}

pub(crate) fn validate_identifier(value: &str) -> Result<(), AdapterHostError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(())
}

fn validate_stored_path(path: &Path) -> Result<(), AdapterHostError> {
    let value = path.to_str().ok_or(AdapterHostError::InstallationInvalid)?;
    if !path.is_absolute() || path.file_name().is_none() || value.chars().any(char::is_control) {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(())
}

fn validate_direct_target(value: Option<&str>) -> Result<(), AdapterHostError> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
    }) {
        return Err(AdapterHostError::InstallationInvalid);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::fs;

    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};

    use super::{CatalogDocument, InstallationCatalog};
    use crate::model::RegisteredInstallation;

    #[test]
    fn legacy_catalog_is_readable_and_next_write_upgrades_the_format() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let path = directory.path().join("installations.json");
        let legacy = RegisteredInstallation {
            adapter_id: "legacy".to_owned(),
            client: AdapterClient::Mihomo,
            client_version: AdapterVersion::new(1, 19, 16),
            executable_path: directory.path().join("mihomo"),
            managed_rules_path: directory.path().join("nonproxy.yaml"),
            main_configuration_path: None,
            direct_target: None,
        };
        let bytes = serde_json::to_vec(&CatalogDocument {
            format_version: 1,
            installations: vec![legacy],
        })
        .unwrap_or_else(|error| panic!("旧目录编码失败: {error}"));
        fs::write(&path, bytes).unwrap_or_else(|error| panic!("旧目录写入失败: {error}"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .unwrap_or_else(|error| panic!("旧目录权限设置失败: {error}"));

        let catalog = InstallationCatalog::open(&path)
            .unwrap_or_else(|error| panic!("旧目录打开失败: {error}"));
        let listed = catalog
            .list()
            .unwrap_or_else(|error| panic!("旧目录读取失败: {error}"));
        assert_eq!(listed.len(), 1);
        assert!(listed[0].main_configuration_path.is_none());
        catalog
            .register(RegisteredInstallation {
                adapter_id: "current".to_owned(),
                client: AdapterClient::Mihomo,
                client_version: AdapterVersion::new(1, 19, 16),
                executable_path: directory.path().join("mihomo-current"),
                managed_rules_path: directory.path().join("nonproxy-current.yaml"),
                main_configuration_path: Some(directory.path().join("config.yaml")),
                direct_target: None,
            })
            .unwrap_or_else(|error| panic!("新目录项登记失败: {error}"));

        let document: serde_json::Value = serde_json::from_slice(
            &fs::read(&path).unwrap_or_else(|error| panic!("升级目录读取失败: {error}")),
        )
        .unwrap_or_else(|error| panic!("升级目录解析失败: {error}"));
        assert_eq!(document["format_version"], 2);
    }
}
