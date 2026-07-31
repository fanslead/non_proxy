#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use tokio::net::{UnixListener, UnixStream};

use crate::AdapterHostError;

pub(crate) async fn bind_private_socket(
    path: &Path,
) -> Result<(UnixListener, SocketGuard), AdapterHostError> {
    prepare_socket_path(path).await?;
    let listener = UnixListener::bind(path).map_err(AdapterHostError::File)?;
    if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _cleanup = fs::remove_file(path);
        return Err(AdapterHostError::File(error));
    }
    let metadata = fs::symlink_metadata(path).map_err(AdapterHostError::File)?;
    Ok((
        listener,
        SocketGuard {
            path: path.to_path_buf(),
            device: metadata.dev(),
            inode: metadata.ino(),
        },
    ))
}

async fn prepare_socket_path(path: &Path) -> Result<(), AdapterHostError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() => {
            Err(AdapterHostError::Configuration)
        }
        Ok(_) => match UnixStream::connect(path).await {
            Ok(_) => Err(AdapterHostError::Configuration),
            Err(_) => fs::remove_file(path).map_err(AdapterHostError::File),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AdapterHostError::File(error)),
    }
}

pub(crate) struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket()
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            let _cleanup = fs::remove_file(&self.path);
        }
    }
}
