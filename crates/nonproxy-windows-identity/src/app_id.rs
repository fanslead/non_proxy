use sha2::{Digest, Sha256};

const MAXIMUM_STABLE_ID_CHARACTERS: usize = 2_048;
const MAXIMUM_WFP_APP_ID_BYTES: usize = 4_096;

#[must_use]
pub fn decode_wfp_app_id(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAXIMUM_WFP_APP_ID_BYTES || !bytes.len().is_multiple_of(2)
    {
        return None;
    }

    let words = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    let end = words
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(words.len());
    if words[end..].iter().any(|value| *value != 0) {
        return None;
    }
    let app_id = String::from_utf16(&words[..end]).ok()?;
    if app_id.is_empty()
        || app_id.chars().count() > MAXIMUM_STABLE_ID_CHARACTERS
        || app_id.chars().any(char::is_control)
    {
        None
    } else {
        Some(app_id)
    }
}

#[must_use]
pub fn certificate_signer_identity(certificate: &[u8]) -> Option<String> {
    if certificate.is_empty() {
        return None;
    }
    let digest = Sha256::digest(certificate);
    Some(format!("cert-sha256:{}", lowercase_hex(&digest)))
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(DIGITS[usize::from(byte >> 4)]));
        value.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{certificate_signer_identity, decode_wfp_app_id};

    #[test]
    fn decodes_null_terminated_canonical_wfp_utf16_identity() {
        let bytes = "\\device\\harddiskvolume3\\apps\\example.exe\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();

        assert_eq!(
            decode_wfp_app_id(&bytes).as_deref(),
            Some("\\device\\harddiskvolume3\\apps\\example.exe")
        );
    }

    #[test]
    fn rejects_odd_invalid_and_oversized_payloads() {
        assert_eq!(decode_wfp_app_id(&[1]), None);
        assert_eq!(decode_wfp_app_id(&[0, 0]), None);
        assert_eq!(decode_wfp_app_id(&[b'a', 0, 0, 0, b'b', 0]), None);
        assert_eq!(decode_wfp_app_id(&vec![b'a'; 4_098]), None);
        let oversized = "a"
            .repeat(2_049)
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(decode_wfp_app_id(&oversized), None);
    }

    #[test]
    fn signer_identity_is_sha256_of_leaf_certificate_der() {
        assert_eq!(
            certificate_signer_identity(&[1, 2, 3]).as_deref(),
            Some("cert-sha256:039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81")
        );
        assert_eq!(certificate_signer_identity(&[]), None);
    }
}
