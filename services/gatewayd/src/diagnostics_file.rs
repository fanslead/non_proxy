use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use crate::diagnostics_export::DiagnosticsExportError;

pub(crate) fn write_private(
    directory: &Path,
    diagnostic_id: &str,
    content: &[u8],
) -> Result<PathBuf, DiagnosticsExportError> {
    prepare_directory(directory)?;
    let final_path = directory.join(format!("nonproxy-{diagnostic_id}.json"));
    let temporary_path = directory.join(format!(".{diagnostic_id}.tmp"));
    reject_existing_path(&final_path)?;
    reject_existing_path(&temporary_path)?;

    let result = write_and_publish(&temporary_path, &final_path, content);
    if result.is_err() {
        let _remove_result = fs::remove_file(&temporary_path);
    }
    result.map(|()| final_path)
}

fn prepare_directory(directory: &Path) -> Result<(), DiagnosticsExportError> {
    let parent = directory
        .parent()
        .ok_or_else(DiagnosticsExportError::unsafe_path)?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(DiagnosticsExportError::file)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(DiagnosticsExportError::unsafe_path());
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(DiagnosticsExportError::unsafe_path());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(directory).map_err(DiagnosticsExportError::file)?;
        }
        Err(error) => return Err(DiagnosticsExportError::file(error)),
    }
    restrict_directory(directory)?;
    Ok(())
}

fn reject_existing_path(path: &Path) -> Result<(), DiagnosticsExportError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(DiagnosticsExportError::collision()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DiagnosticsExportError::file(error)),
    }
}

fn write_and_publish(
    temporary_path: &Path,
    final_path: &Path,
    content: &[u8],
) -> Result<(), DiagnosticsExportError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary_path)
        .map_err(DiagnosticsExportError::file)?;
    restrict_file(temporary_path)?;
    file.write_all(content)
        .map_err(DiagnosticsExportError::file)?;
    file.sync_all().map_err(DiagnosticsExportError::file)?;
    drop(file);
    fs::hard_link(temporary_path, final_path).map_err(DiagnosticsExportError::file)?;
    fs::remove_file(temporary_path).map_err(DiagnosticsExportError::file)?;
    sync_directory(final_path.parent())?;
    Ok(())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), DiagnosticsExportError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(DiagnosticsExportError::file)
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), DiagnosticsExportError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> Result<(), DiagnosticsExportError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(DiagnosticsExportError::file)
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> Result<(), DiagnosticsExportError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(parent: Option<&Path>) -> Result<(), DiagnosticsExportError> {
    let parent = parent.ok_or_else(DiagnosticsExportError::unsafe_path)?;
    let directory = fs::File::open(parent).map_err(DiagnosticsExportError::file)?;
    directory.sync_all().map_err(DiagnosticsExportError::file)
}

#[cfg(not(unix))]
fn sync_directory(_parent: Option<&Path>) -> Result<(), DiagnosticsExportError> {
    Ok(())
}
