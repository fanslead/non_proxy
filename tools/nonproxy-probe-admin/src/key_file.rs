#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use nonproxy_exit_probe::ExitProbeSigner;
use zeroize::Zeroizing;

use crate::AdminError;

const SIGNING_KEY_BYTES: usize = 32;

pub struct KeyMetadata {
    pub key_id: String,
    pub public_key: String,
}

pub fn generate(path: &Path) -> Result<KeyMetadata, AdminError> {
    validate_output_path(path)?;
    let mut secret = Zeroizing::new([0_u8; SIGNING_KEY_BYTES]);
    getrandom::fill(secret.as_mut()).map_err(|_| AdminError::Random)?;
    let signer =
        ExitProbeSigner::from_secret_bytes(secret.as_ref()).map_err(|_| AdminError::SigningKey)?;
    let mut file = create_private_file(path)?;
    file.write_all(secret.as_ref())
        .and_then(|()| file.sync_all())
        .map_err(|_| AdminError::File)?;
    Ok(metadata(&signer))
}

pub fn inspect(path: &Path) -> Result<KeyMetadata, AdminError> {
    let secret = Zeroizing::new(read_private_key(path)?);
    let signer = ExitProbeSigner::from_secret_bytes(secret.as_slice())
        .map_err(|_| AdminError::SigningKey)?;
    Ok(metadata(&signer))
}

fn metadata(signer: &ExitProbeSigner) -> KeyMetadata {
    KeyMetadata {
        key_id: signer.key_id().to_owned(),
        public_key: signer.public_key_base64(),
    }
}

fn validate_output_path(path: &Path) -> Result<(), AdminError> {
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AdminError::File);
    }
    let parent = path.parent().ok_or(AdminError::File)?;
    reject_symbolic_link_components(parent)?;
    let metadata = fs::symlink_metadata(parent).map_err(|_| AdminError::File)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AdminError::File);
    }
    Ok(())
}

fn create_private_file(path: &Path) -> Result<File, AdminError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;

        options.mode(0o600);
    }
    options.open(path).map_err(|_| AdminError::File)
}

fn read_private_key(path: &Path) -> Result<Vec<u8>, AdminError> {
    if !path.is_absolute() {
        return Err(AdminError::File);
    }
    let parent = path.parent().ok_or(AdminError::File)?;
    reject_symbolic_link_components(parent)?;
    let mut file = open_private_file(path)?;
    let metadata = file.metadata().map_err(|_| AdminError::File)?;
    if !metadata.is_file() || metadata.len() != SIGNING_KEY_BYTES as u64 {
        return Err(AdminError::File);
    }
    validate_permissions(&metadata)?;
    let mut secret = Vec::with_capacity(SIGNING_KEY_BYTES);
    file.read_to_end(&mut secret)
        .map_err(|_| AdminError::File)?;
    if secret.len() != SIGNING_KEY_BYTES {
        return Err(AdminError::File);
    }
    Ok(secret)
}

fn open_private_file(path: &Path) -> Result<File, AdminError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;

        options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    #[cfg(not(unix))]
    {
        let metadata = fs::symlink_metadata(path).map_err(|_| AdminError::File)?;
        if metadata.file_type().is_symlink() {
            return Err(AdminError::File);
        }
    }
    options.open(path).map_err(|_| AdminError::File)
}

#[cfg(target_os = "linux")]
fn reject_symbolic_link_components(path: &Path) -> Result<(), AdminError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current).map_err(|_| AdminError::File)?;
        if metadata.file_type().is_symlink() {
            return Err(AdminError::File);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn reject_symbolic_link_components(_path: &Path) -> Result<(), AdminError> {
    Ok(())
}

#[cfg(unix)]
fn validate_permissions(metadata: &fs::Metadata) -> Result<(), AdminError> {
    use std::os::unix::fs::PermissionsExt as _;

    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(AdminError::File);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_permissions(_metadata: &fs::Metadata) -> Result<(), AdminError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::fs;

    use super::{generate, inspect};

    #[test]
    fn key_generation_is_private_non_overwriting_and_reproducibly_inspectable() {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("密钥工具测试目录创建失败: {error}"));
        let path = directory.path().join("signing-key.bin");

        let generated = generate(&path).unwrap_or_else(|error| panic!("测试密钥生成失败: {error}"));
        let inspected = inspect(&path).unwrap_or_else(|error| panic!("测试密钥读取失败: {error}"));

        assert_eq!(generated.key_id, inspected.key_id);
        assert_eq!(generated.public_key, inspected.public_key);
        assert_eq!(
            std::fs::metadata(&path)
                .unwrap_or_else(|error| panic!("测试密钥 metadata 读取失败: {error}"))
                .len(),
            32
        );
        assert!(generate(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn inspect_rejects_loose_permissions_and_symbolic_link_paths() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("密钥权限测试目录创建失败: {error}"));
        let key_path = directory.path().join("signing-key.bin");
        generate(&key_path).unwrap_or_else(|error| panic!("测试密钥生成失败: {error}"));

        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o644))
            .unwrap_or_else(|error| panic!("测试密钥权限修改失败: {error}"));
        assert!(inspect(&key_path).is_err());
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
            .unwrap_or_else(|error| panic!("测试密钥权限恢复失败: {error}"));

        let linked_key = directory.path().join("linked-key.bin");
        symlink(&key_path, &linked_key)
            .unwrap_or_else(|error| panic!("测试密钥符号链接创建失败: {error}"));
        assert!(inspect(&linked_key).is_err());

        let linked_directory = directory.path().join("linked-directory");
        symlink(directory.path(), &linked_directory)
            .unwrap_or_else(|error| panic!("测试目录符号链接创建失败: {error}"));
        assert!(generate(&linked_directory.join("new-key.bin")).is_err());
    }
}
