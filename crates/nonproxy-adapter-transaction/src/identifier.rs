use crate::AdapterTransactionError;

pub(crate) fn validate_identifier(value: &str) -> Result<(), AdapterTransactionError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(AdapterTransactionError::ChangeIdInvalid);
    }
    Ok(())
}
