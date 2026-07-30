use std::{fmt, str::FromStr};

use crate::ModelError;

const MAX_IDENTIFIER_LENGTH: usize = 128;

macro_rules! define_identifier {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ModelError> {
                let value = value.into();
                validate_identifier(&value)?;
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

        impl FromStr for $name {
            type Err = ModelError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
    };
}

define_identifier!(PolicyId);
define_identifier!(RuleId);
define_identifier!(OutboundId);
define_identifier!(NetworkProfileId);

fn validate_identifier(value: &str) -> Result<(), ModelError> {
    if value.is_empty() || value.trim() != value {
        return Err(ModelError::EmptyIdentifier);
    }
    if value.len() > MAX_IDENTIFIER_LENGTH {
        return Err(ModelError::IdentifierTooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(ModelError::IdentifierContainsControl);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(ModelError::InvalidIdentifierCharacter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_ambiguous_whitespace() {
        assert!(matches!(
            PolicyId::new(" policy-a"),
            Err(ModelError::EmptyIdentifier)
        ));
        assert!(matches!(
            PolicyId::new("policy-a\n"),
            Err(ModelError::EmptyIdentifier)
        ));
    }

    #[test]
    fn identifier_preserves_stable_value() {
        let result = PolicyId::new("policy-a");
        let Ok(identifier) = result else {
            panic!("有效标识符应当创建成功: {result:?}");
        };

        assert_eq!(identifier.as_str(), "policy-a");
        assert_eq!(identifier.to_string(), "policy-a");
    }

    #[test]
    fn identifier_rejects_path_and_log_delimiters() {
        assert!(matches!(
            PolicyId::new("../policy"),
            Err(ModelError::InvalidIdentifierCharacter)
        ));
        assert!(matches!(
            PolicyId::new("策略一"),
            Err(ModelError::InvalidIdentifierCharacter)
        ));
    }
}
