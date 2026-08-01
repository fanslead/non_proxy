use std::{
    fs::{self, OpenOptions},
    io,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr,
};

use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW, ReplaceFileW};

use crate::validate_regular_file;

pub fn replace_file_atomically(source: &Path, target: &Path) -> io::Result<()> {
    if source == target || source.parent().is_none() || source.parent() != target.parent() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 原子替换必须位于同一目录",
        ));
    }
    validate_regular_file(source)?;
    let target_exists = match fs::symlink_metadata(target) {
        Ok(_) => {
            validate_regular_file(target)?;
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };
    let encoded_source = wide_path(source)?;
    let encoded_target = wide_path(target)?;
    // SAFETY: 路径均为 NUL 结尾 UTF-16，源和目标位于同一目录；调用期间缓冲区有效。
    let replaced = unsafe {
        if target_exists {
            ReplaceFileW(
                encoded_target.as_ptr(),
                encoded_source.as_ptr(),
                ptr::null(),
                0,
                ptr::null(),
                ptr::null(),
            )
        } else {
            MoveFileExW(
                encoded_source.as_ptr(),
                encoded_target.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if replaced == 0 {
        return Err(io::Error::last_os_error());
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(target)?
        .sync_all()
}

fn wide_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if encoded.is_empty() || encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 原子替换路径无效",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::replace_file_atomically;
    use crate::acl::{protect_current_user_file, verify_current_user_file};

    #[test]
    fn moves_new_file_and_replaces_existing_file_without_empty_window() {
        let directory =
            tempfile::tempdir().unwrap_or_else(|error| panic!("临时目录创建失败: {error}"));
        let target = directory.path().join("managed.rules");
        let first = directory.path().join("first.tmp");
        fs::write(&first, b"first").unwrap_or_else(|error| panic!("首次候选写入失败: {error}"));
        replace_file_atomically(&first, &target)
            .unwrap_or_else(|error| panic!("首次原子移动失败: {error}"));
        assert_eq!(fs::read(&target).ok().as_deref(), Some(b"first".as_slice()));
        protect_current_user_file(&target)
            .unwrap_or_else(|error| panic!("目标 DACL 设置失败: {error}"));

        let second = directory.path().join("second.tmp");
        fs::write(&second, b"second").unwrap_or_else(|error| panic!("替换候选写入失败: {error}"));
        replace_file_atomically(&second, &target)
            .unwrap_or_else(|error| panic!("既有文件原子替换失败: {error}"));
        assert_eq!(
            fs::read(&target).ok().as_deref(),
            Some(b"second".as_slice())
        );
        verify_current_user_file(&target)
            .unwrap_or_else(|error| panic!("ReplaceFileW 未保留目标 DACL: {error}"));
        assert!(!first.exists());
        assert!(!second.exists());
    }
}
