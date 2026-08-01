use std::mem::size_of;

const PACKAGE_SID_BYTES: usize = 40;
const PACKAGE_SUBAUTHORITY_COUNT: usize = 8;
const PACKAGE_AUTHORITY: u64 = 15;
const PACKAGE_RID: u32 = 2;
const PUBLISHER_ID_CHARACTERS: usize = 13;

#[must_use]
pub fn decode_package_sid(bytes: &[u8]) -> Option<String> {
    if bytes.len() != PACKAGE_SID_BYTES
        || bytes[0] != 1
        || usize::from(bytes[1]) != PACKAGE_SUBAUTHORITY_COUNT
        || identifier_authority(bytes) != PACKAGE_AUTHORITY
        || subauthority(bytes, 0) != Some(PACKAGE_RID)
    {
        return None;
    }
    let mut value = String::from("S-1-15");
    for index in 0..PACKAGE_SUBAUTHORITY_COUNT {
        value.push('-');
        value.push_str(&subauthority(bytes, index)?.to_string());
    }
    Some(value)
}

#[must_use]
pub fn package_stable_identity(bytes: &[u8]) -> Option<String> {
    decode_package_sid(bytes).map(|sid| format!("package-sid:{sid}"))
}

#[must_use]
pub fn package_publisher_signer_identity(publisher_id: &str) -> Option<String> {
    if publisher_id.len() != PUBLISHER_ID_CHARACTERS
        || publisher_id.trim() != publisher_id
        || !publisher_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(format!(
        "package-publisher-id:{}",
        publisher_id.to_ascii_lowercase()
    ))
}

fn identifier_authority(bytes: &[u8]) -> u64 {
    bytes[2..8]
        .iter()
        .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte))
}

fn subauthority(bytes: &[u8], index: usize) -> Option<u32> {
    let offset = 8_usize.checked_add(index.checked_mul(size_of::<u32>())?)?;
    let raw: [u8; 4] = bytes
        .get(offset..offset + size_of::<u32>())?
        .try_into()
        .ok()?;
    Some(u32::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_identity_requires_application_package_sid_shape() {
        let sid = package_sid([2, 1, 2, 3, 4, 5, 6, 7]);

        assert_eq!(
            package_stable_identity(&sid).as_deref(),
            Some("package-sid:S-1-15-2-1-2-3-4-5-6-7")
        );
        assert_eq!(
            package_publisher_signer_identity("8WEKYB3D8BBWE").as_deref(),
            Some("package-publisher-id:8wekyb3d8bbwe")
        );
        assert_eq!(package_stable_identity(&package_sid([3; 8])), None);
        assert_eq!(package_stable_identity(&sid[..sid.len() - 1]), None);
        assert_eq!(package_publisher_signer_identity("publisher"), None);
        assert_eq!(package_publisher_signer_identity("8wekyb3d8bbw_"), None);
        assert_eq!(package_publisher_signer_identity(" 8wekyb3d8bbwe"), None);
    }

    fn package_sid(subauthorities: [u32; 8]) -> Vec<u8> {
        let mut bytes = vec![1, 8, 0, 0, 0, 0, 0, 15];
        bytes.extend(subauthorities.into_iter().flat_map(u32::to_le_bytes));
        bytes
    }
}
