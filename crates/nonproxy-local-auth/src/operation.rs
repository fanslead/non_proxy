use crate::LocalAuthError;

const MAXIMUM_OPERATION_ID_BYTES: usize = 128;

pub fn validate_operation_id(value: &str) -> Result<(), LocalAuthError> {
    if value.is_empty()
        || value.len() > MAXIMUM_OPERATION_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(LocalAuthError::OperationIdInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_operation_id;

    #[test]
    fn operation_ids_are_bounded_and_path_safe() {
        assert!(validate_operation_id("desktop:adapter-42").is_ok());
        assert!(validate_operation_id("").is_err());
        assert!(validate_operation_id("../escape").is_err());
        assert!(validate_operation_id(&"a".repeat(129)).is_err());
    }
}
