use sha2::{Digest, Sha256};

use crate::AdapterTransactionError;

pub(crate) fn stable_identifier(prefix: &str, parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(part);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    format!("{prefix}-{}", &encode_hash(&digest)[..32])
}

pub(crate) fn encode_hash(hash: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in hash {
        use std::fmt::Write as _;
        let _result = write!(output, "{byte:02x}");
    }
    output
}

pub(crate) fn decode_hash(value: &str) -> Result<[u8; 32], AdapterTransactionError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AdapterTransactionError::StateCorrupt);
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16)
            .map_err(|_| AdapterTransactionError::StateCorrupt)?;
    }
    Ok(output)
}
