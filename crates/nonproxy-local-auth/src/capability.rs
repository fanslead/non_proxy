use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::LocalAuthError;

pub const SESSION_TOKEN_LENGTH: usize = 32;
const TEMPORARY_FILE_ATTEMPTS: usize = 4;

#[derive(Clone)]
pub struct SessionCapability {
    token: [u8; SESSION_TOKEN_LENGTH],
}

impl SessionCapability {
    pub fn create(state_directory: &Path, file_name: &str) -> Result<Self, LocalAuthError> {
        validate_state_directory(state_directory)?;
        validate_file_name(file_name)?;
        let mut token = [0_u8; SESSION_TOKEN_LENGTH];
        getrandom::fill(&mut token).map_err(LocalAuthError::Random)?;
        let capability = Self { token };
        capability.write_to(state_directory, file_name)?;
        Ok(capability)
    }

    #[must_use]
    pub const fn from_token(token: [u8; SESSION_TOKEN_LENGTH]) -> Self {
        Self { token }
    }

    #[must_use]
    pub fn matches(&self, actual: &[u8]) -> bool {
        if actual.len() != SESSION_TOKEN_LENGTH {
            return false;
        }
        self.token
            .iter()
            .zip(actual)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
    }

    #[must_use]
    pub const fn token(&self) -> &[u8; SESSION_TOKEN_LENGTH] {
        &self.token
    }

    fn write_to(&self, state_directory: &Path, file_name: &str) -> Result<(), LocalAuthError> {
        let target = state_directory.join(file_name);
        reject_unsafe_target(&target)?;
        let (temporary, mut file) = create_temporary_file(state_directory, file_name)?;
        let write_result = file
            .write_all(&self.token)
            .and_then(|()| file.sync_all())
            .map_err(LocalAuthError::File);
        drop(file);
        if let Err(error) = write_result {
            let _cleanup = fs::remove_file(&temporary);
            return Err(error);
        }
        if let Err(error) = replace_capability_file(&temporary, &target) {
            cleanup_failed_replacement(&temporary);
            return Err(error);
        }
        sync_directory(state_directory)
    }
}

fn validate_state_directory(path: &Path) -> Result<(), LocalAuthError> {
    if !path.is_absolute() {
        return Err(LocalAuthError::StateDirectoryInvalid);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| LocalAuthError::StateDirectoryInvalid)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LocalAuthError::StateDirectoryInvalid);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(LocalAuthError::StateDirectoryInvalid);
    }
    Ok(())
}

fn validate_file_name(value: &str) -> Result<(), LocalAuthError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(LocalAuthError::FileNameInvalid);
    }
    Ok(())
}

fn create_temporary_file(
    state_directory: &Path,
    file_name: &str,
) -> Result<(PathBuf, File), LocalAuthError> {
    let mut collision = None;
    for _attempt in 0..TEMPORARY_FILE_ATTEMPTS {
        let mut nonce = [0_u8; 8];
        getrandom::fill(&mut nonce).map_err(LocalAuthError::Random)?;
        let suffix = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = state_directory.join(format!(".{file_name}.{suffix}.tmp"));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                collision = Some(error);
            }
            Err(error) => return Err(LocalAuthError::File(error)),
        }
    }
    Err(LocalAuthError::File(collision.unwrap_or_else(|| {
        std::io::Error::other("temporary capability path allocation exhausted")
    })))
}

fn reject_unsafe_target(path: &Path) -> Result<(), LocalAuthError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(LocalAuthError::CapabilityPathInvalid)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(LocalAuthError::File(error)),
    }
}

#[cfg(unix)]
fn replace_capability_file(source: &Path, target: &Path) -> Result<(), LocalAuthError> {
    fs::rename(source, target).map_err(LocalAuthError::File)
}

#[cfg(windows)]
fn cleanup_failed_replacement(_path: &Path) {}

#[cfg(not(windows))]
fn cleanup_failed_replacement(path: &Path) {
    let _cleanup = fs::remove_file(path);
}

#[cfg(windows)]
fn replace_capability_file(source: &Path, target: &Path) -> Result<(), LocalAuthError> {
    nonproxy_windows_security::replace_file_atomically(source, target).map_err(LocalAuthError::File)
}

#[cfg(not(any(unix, windows)))]
fn replace_capability_file(source: &Path, target: &Path) -> Result<(), LocalAuthError> {
    if target.exists() {
        fs::remove_file(target).map_err(LocalAuthError::File)?;
    }
    fs::rename(source, target).map_err(LocalAuthError::File)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), LocalAuthError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(LocalAuthError::File)
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), LocalAuthError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{SESSION_TOKEN_LENGTH, SessionCapability};

    #[test]
    fn creates_private_rotating_capability_and_matches_in_constant_shape() {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
                .unwrap_or_else(|error| panic!("临时目录权限设置失败: {error}"));
        }
        let first = SessionCapability::create(directory.path(), "adapter.capability")
            .unwrap_or_else(|error| panic!("首次能力创建失败: {error}"));
        let second = SessionCapability::create(directory.path(), "adapter.capability")
            .unwrap_or_else(|error| panic!("能力轮换失败: {error}"));
        let stored = fs::read(directory.path().join("adapter.capability"))
            .unwrap_or_else(|error| panic!("能力文件读取失败: {error}"));

        assert_ne!(first.token(), second.token());
        assert_eq!(stored.len(), SESSION_TOKEN_LENGTH);
        assert!(second.matches(&stored));
        assert!(!second.matches(&stored[..SESSION_TOKEN_LENGTH - 1]));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(directory.path().join("adapter.capability"))
                .unwrap_or_else(|error| panic!("能力文件元数据读取失败: {error}"))
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symbolic_link_target() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("临时目录权限设置失败: {error}"));
        let outside = directory.path().join("outside");
        fs::write(&outside, b"preserve")
            .unwrap_or_else(|error| panic!("外部文件写入失败: {error}"));
        symlink(&outside, directory.path().join("adapter.capability"))
            .unwrap_or_else(|error| panic!("测试符号链接创建失败: {error}"));

        assert!(SessionCapability::create(directory.path(), "adapter.capability").is_err());
        assert_eq!(
            fs::read(outside).unwrap_or_else(|error| panic!("外部文件读取失败: {error}")),
            b"preserve"
        );
    }
}
