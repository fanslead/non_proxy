use std::{fs, io, os::windows::fs::MetadataExt, path::Path};

use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

pub fn validate_regular_directory(path: &Path) -> io::Result<()> {
    validate(path, true)
}

pub fn validate_regular_file(path: &Path) -> io::Result<()> {
    validate(path, false)
}

fn validate(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 安全路径不是常规对象",
        ));
    }
    Ok(())
}
