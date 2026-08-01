use std::io;

use nonproxy_windows_security::validate_interactive_user_sid;

const PIPE_PREFIX: &str = r"\\.\pipe\NonProxy.";
const MAXIMUM_PIPE_NAME_BYTES: usize = 160;
const MAXIMUM_SDDL_BYTES: usize = 1_024;

pub fn adapter_pipe_name_for_user_sid(value: &str) -> io::Result<String> {
    validate_interactive_user_sid(value)?;
    let pipe = format!(r"{PIPE_PREFIX}Adapter.{value}");
    validate_nonproxy_pipe_name(&pipe)?;
    Ok(pipe)
}

pub fn adapter_pipe_sddl_for_user_sid(value: &str) -> io::Result<String> {
    validate_interactive_user_sid(value)?;
    let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{value})");
    validate_pipe_sddl(&sddl)?;
    Ok(sddl)
}

pub fn validate_nonproxy_pipe_name(value: &str) -> io::Result<()> {
    let suffix = value.strip_prefix(PIPE_PREFIX).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 命名管道必须位于 NonProxy 本地命名空间",
        )
    })?;
    if suffix.is_empty()
        || value.len() > MAXIMUM_PIPE_NAME_BYTES
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 命名管道名称无效",
        ));
    }
    Ok(())
}

pub fn validate_pipe_sddl(value: &str) -> io::Result<()> {
    if value.len() > MAXIMUM_SDDL_BYTES || !value.starts_with("D:") || value.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows 命名管道 DACL 无效",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PIPE_PREFIX, adapter_pipe_name_for_user_sid, adapter_pipe_sddl_for_user_sid,
        validate_nonproxy_pipe_name, validate_pipe_sddl,
    };
    use nonproxy_windows_security::validate_interactive_user_sid;

    const USER_SID: &str = "S-1-5-21-1000-2000-3000-1001";

    #[test]
    fn adapter_pipe_and_sddl_bind_one_canonical_user_sid() {
        assert_eq!(
            adapter_pipe_name_for_user_sid(USER_SID).ok().as_deref(),
            Some(r"\\.\pipe\NonProxy.Adapter.S-1-5-21-1000-2000-3000-1001")
        );
        assert_eq!(
            adapter_pipe_sddl_for_user_sid(USER_SID).ok().as_deref(),
            Some("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;S-1-5-21-1000-2000-3000-1001)")
        );
    }

    #[test]
    fn adapter_identity_rejects_services_and_noncanonical_sid_text() {
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

    #[test]
    fn pipe_names_are_bounded_to_the_product_namespace() {
        let maximum = format!(
            "{PIPE_PREFIX}{}",
            "a".repeat(160_usize.saturating_sub(PIPE_PREFIX.len()))
        );
        assert!(validate_nonproxy_pipe_name(&maximum).is_ok());
        for invalid in [
            r"\\.\pipe\Other.Control".to_owned(),
            r"\\.\pipe\NonProxy.Invalid\Child".to_owned(),
            format!("{maximum}a"),
        ] {
            assert!(validate_nonproxy_pipe_name(&invalid).is_err());
        }
    }

    #[test]
    fn pipe_sddl_rejects_wrong_section_nul_and_unbounded_values() {
        assert!(validate_pipe_sddl("D:P(A;;GA;;;SY)").is_ok());
        for invalid in [
            "O:SY".to_owned(),
            "D:\0P".to_owned(),
            format!("D:{}", "A".repeat(1_023)),
        ] {
            assert!(validate_pipe_sddl(&invalid).is_err());
        }
    }
}
