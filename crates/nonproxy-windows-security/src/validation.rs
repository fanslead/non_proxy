use std::io;

const MAXIMUM_SID_BYTES: usize = 184;
const MAXIMUM_SID_IDENTIFIER_AUTHORITY: u64 = 0x0000_FFFF_FFFF_FFFF;

pub fn validate_interactive_user_sid(value: &str) -> io::Result<()> {
    let parts = value.split('-').collect::<Vec<_>>();
    let structurally_valid = (4..=18).contains(&parts.len())
        && parts[0] == "S"
        && parts[1] == "1"
        && parts[2..].iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part == &"0" || !part.starts_with('0'))
        })
        && parts[2]
            .parse::<u64>()
            .is_ok_and(|authority| authority <= MAXIMUM_SID_IDENTIFIER_AUTHORITY)
        && parts[3..].iter().all(|part| part.parse::<u32>().is_ok());
    let service_identity =
        matches!(value, "S-1-5-18" | "S-1-5-19" | "S-1-5-20") || value.starts_with("S-1-5-80-");
    if value.len() > MAXIMUM_SID_BYTES || !structurally_valid || service_identity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 操作必须绑定规范交互用户 SID",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_interactive_user_sid;

    #[test]
    fn accepts_users_and_rejects_services_or_noncanonical_text() {
        assert!(validate_interactive_user_sid("S-1-5-21-1000-2000-3000-1001").is_ok());
        for invalid in [
            "S-1-5-18",
            "S-1-5-19",
            "S-1-5-20",
            "S-1-5-80-1234",
            "s-1-5-21-1001",
            "S-01-5-21-1001",
            "S-1-05-21-1001",
            "S-1-5-21--1001",
            "S-1-5",
            "S-1-281474976710656-1",
            "S-1-5-4294967296",
        ] {
            assert!(validate_interactive_user_sid(invalid).is_err());
        }
    }
}
