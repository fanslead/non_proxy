use std::fmt;

use crate::LearningError;

const MAX_IDENTIFIER_LENGTH: usize = 128;

macro_rules! learning_identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, LearningError> {
                let value = value.into();
                validate(&value)?;
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

learning_identifier!(LearningSessionId);
learning_identifier!(BrowserContextId);
learning_identifier!(ObservationId);
learning_identifier!(ConfirmationId);

fn validate(value: &str) -> Result<(), LearningError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_LENGTH
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(LearningError::InvalidIdentifier);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_reject_paths_and_whitespace() {
        assert!(LearningSessionId::new("../session").is_err());
        assert!(BrowserContextId::new("tab 1").is_err());
        assert!(ObservationId::new("").is_err());
        assert!(ConfirmationId::new("confirm/path").is_err());
    }
}
