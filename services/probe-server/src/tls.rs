use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
    sync::Arc,
};

use rustls::{ServerConfig, pki_types::PrivateKeyDer};
use zeroize::Zeroizing;

use crate::error::ProbeServerError;

const MAXIMUM_SECRET_FILE_BYTES: u64 = 64 * 1024;

pub fn load_server_config(
    certificate_path: &Path,
    key_path: &Path,
) -> Result<Arc<ServerConfig>, ProbeServerError> {
    let certificate_file = File::open(certificate_path).map_err(|_| ProbeServerError::File)?;
    let certificates = rustls_pemfile::certs(&mut BufReader::new(certificate_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProbeServerError::Tls)?;
    if certificates.is_empty() {
        return Err(ProbeServerError::Tls);
    }
    let key_bytes = Zeroizing::new(read_secret_file(key_path, None)?);
    let key = rustls_pemfile::private_key(&mut BufReader::new(key_bytes.as_slice()))
        .map_err(|_| ProbeServerError::Tls)?
        .ok_or(ProbeServerError::Tls)?;
    build_server_config(certificates, key)
}

fn build_server_config(
    certificates: Vec<rustls::pki_types::CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, ProbeServerError> {
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|_| ProbeServerError::Tls)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

pub fn read_secret_file(
    path: &Path,
    exact_length: Option<usize>,
) -> Result<Vec<u8>, ProbeServerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ProbeServerError::File)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAXIMUM_SECRET_FILE_BYTES
    {
        return Err(ProbeServerError::File);
    }
    validate_secret_permissions(&metadata)?;
    let mut file = File::open(path).map_err(|_| ProbeServerError::File)?;
    let mut bytes =
        Vec::with_capacity(usize::try_from(metadata.len()).map_err(|_| ProbeServerError::File)?);
    file.read_to_end(&mut bytes)
        .map_err(|_| ProbeServerError::File)?;
    if exact_length.is_some_and(|expected| bytes.len() != expected) {
        return Err(ProbeServerError::File);
    }
    Ok(bytes)
}

#[cfg(unix)]
fn validate_secret_permissions(metadata: &fs::Metadata) -> Result<(), ProbeServerError> {
    use std::os::unix::fs::PermissionsExt as _;

    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ProbeServerError::File);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_secret_permissions(_metadata: &fs::Metadata) -> Result<(), ProbeServerError> {
    Ok(())
}
