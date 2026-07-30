use std::{
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use tokio::net::{UnixListener, UnixStream};

use crate::GatewayError;

pub(crate) async fn bind_private_socket(
    path: &Path,
    role: SocketRole,
) -> Result<(UnixListener, SocketGuard), GatewayError> {
    prepare_socket_path(path, role).await?;
    let listener = UnixListener::bind(path).map_err(|source| GatewayError::Io {
        operation: role.bind_operation(),
        source,
    })?;
    if let Err(source) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _cleanup_result = fs::remove_file(path);
        return Err(GatewayError::Io {
            operation: role.restrict_operation(),
            source,
        });
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(source) => {
            drop(listener);
            let _cleanup_result = fs::remove_file(path);
            return Err(GatewayError::Io {
                operation: role.metadata_operation(),
                source,
            });
        }
    };
    let guard = SocketGuard {
        path: path.to_path_buf(),
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    Ok((listener, guard))
}

async fn prepare_socket_path(path: &Path, role: SocketRole) -> Result<(), GatewayError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(GatewayError::InvalidLocalPath(role.symlink_message()))
        }
        Ok(metadata) if !metadata.file_type().is_socket() => {
            Err(GatewayError::InvalidLocalPath(role.occupied_message()))
        }
        Ok(_) => match UnixStream::connect(path).await {
            Ok(_) => Err(GatewayError::InvalidLocalPath(role.active_message())),
            Err(_) => fs::remove_file(path).map_err(|source| GatewayError::Io {
                operation: role.remove_operation(),
                source,
            }),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(GatewayError::Io {
            operation: role.check_operation(),
            source,
        }),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SocketRole {
    Control,
    Flow,
}

impl SocketRole {
    const fn symlink_message(self) -> &'static str {
        match self {
            Self::Control => "控制套接字不能是符号链接",
            Self::Flow => "数据套接字不能是符号链接",
        }
    }

    const fn occupied_message(self) -> &'static str {
        match self {
            Self::Control => "控制套接字路径已被普通文件占用",
            Self::Flow => "数据套接字路径已被普通文件占用",
        }
    }

    const fn active_message(self) -> &'static str {
        match self {
            Self::Control => "另一个 gatewayd 控制服务已在运行",
            Self::Flow => "另一个 gatewayd 数据服务已在运行",
        }
    }

    const fn check_operation(self) -> &'static str {
        match self {
            Self::Control => "检查控制套接字路径",
            Self::Flow => "检查数据套接字路径",
        }
    }

    const fn remove_operation(self) -> &'static str {
        match self {
            Self::Control => "移除失效控制套接字",
            Self::Flow => "移除失效数据套接字",
        }
    }

    const fn bind_operation(self) -> &'static str {
        match self {
            Self::Control => "绑定控制套接字",
            Self::Flow => "绑定数据套接字",
        }
    }

    const fn restrict_operation(self) -> &'static str {
        match self {
            Self::Control => "限制控制套接字权限",
            Self::Flow => "限制数据套接字权限",
        }
    }

    const fn metadata_operation(self) -> &'static str {
        match self {
            Self::Control => "读取控制套接字标识",
            Self::Flow => "读取数据套接字标识",
        }
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
            let _result = fs::remove_file(&self.path);
        }
    }
}
