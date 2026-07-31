use std::path::{Path, PathBuf};

use crate::{AdapterHostError, path_validation::canonical_executable};

pub(crate) fn surge_cli(executable: &Path) -> Result<PathBuf, AdapterHostError> {
    let contents = executable
        .parent()
        .and_then(Path::parent)
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("Contents"))
        .ok_or(AdapterHostError::InstallationInvalid)?;
    canonical_executable(&contents.join("Applications/surge-cli"))
}
