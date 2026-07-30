use crate::ModelError;

const MAX_IDENTITY_FIELD_LENGTH: usize = 512;
const UNKNOWN_APP_STABLE_ID: &str = "unknown-app";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Platform {
    MacOs,
    Windows,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppIdentity {
    platform: Platform,
    stable_id: String,
    signer_id: Option<String>,
    executable_hash: Option<Vec<u8>>,
    executable_path_hint: Option<String>,
    display_name: Option<String>,
    parent_stable_id: Option<String>,
    helper_group_id: Option<String>,
}

impl AppIdentity {
    pub fn new(platform: Platform, stable_id: impl Into<String>) -> Result<Self, ModelError> {
        let stable_id = stable_id.into();
        validate_required_field(&stable_id)?;
        Ok(Self {
            platform,
            stable_id,
            signer_id: None,
            executable_hash: None,
            executable_path_hint: None,
            display_name: None,
            parent_stable_id: None,
            helper_group_id: None,
        })
    }

    #[must_use]
    pub fn unknown(platform: Platform) -> Self {
        Self {
            platform,
            stable_id: UNKNOWN_APP_STABLE_ID.to_owned(),
            signer_id: None,
            executable_hash: None,
            executable_path_hint: None,
            display_name: None,
            parent_stable_id: None,
            helper_group_id: None,
        }
    }

    pub fn with_signer_id(mut self, signer_id: impl Into<String>) -> Result<Self, ModelError> {
        self.signer_id = Some(validate_optional_field(signer_id.into())?);
        Ok(self)
    }

    pub fn with_executable_hash(mut self, executable_hash: Vec<u8>) -> Result<Self, ModelError> {
        if executable_hash.is_empty() {
            return Err(ModelError::InvalidAppIdentityField);
        }
        if executable_hash.len() > MAX_IDENTITY_FIELD_LENGTH {
            return Err(ModelError::AppIdentityFieldTooLong);
        }
        self.executable_hash = Some(executable_hash);
        Ok(self)
    }

    pub fn with_path_hint(mut self, path: impl Into<String>) -> Result<Self, ModelError> {
        self.executable_path_hint = Some(validate_optional_field(path.into())?);
        Ok(self)
    }

    pub fn with_display_name(
        mut self,
        display_name: impl Into<String>,
    ) -> Result<Self, ModelError> {
        self.display_name = Some(validate_optional_field(display_name.into())?);
        Ok(self)
    }

    pub fn with_parent_stable_id(
        mut self,
        parent_stable_id: impl Into<String>,
    ) -> Result<Self, ModelError> {
        self.parent_stable_id = Some(validate_optional_field(parent_stable_id.into())?);
        Ok(self)
    }

    pub fn with_helper_group_id(
        mut self,
        helper_group_id: impl Into<String>,
    ) -> Result<Self, ModelError> {
        self.helper_group_id = Some(validate_optional_field(helper_group_id.into())?);
        Ok(self)
    }

    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    #[must_use]
    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }

    #[must_use]
    pub fn signer_id(&self) -> Option<&str> {
        self.signer_id.as_deref()
    }

    #[must_use]
    pub fn executable_hash(&self) -> Option<&[u8]> {
        self.executable_hash.as_deref()
    }

    #[must_use]
    pub fn executable_path_hint(&self) -> Option<&str> {
        self.executable_path_hint.as_deref()
    }

    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    #[must_use]
    pub fn parent_stable_id(&self) -> Option<&str> {
        self.parent_stable_id.as_deref()
    }

    #[must_use]
    pub fn helper_group_id(&self) -> Option<&str> {
        self.helper_group_id.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppMatcher {
    platform: Platform,
    stable_id: String,
    signer_id: Option<String>,
    include_helpers: bool,
}

impl AppMatcher {
    pub fn new(platform: Platform, stable_id: impl Into<String>) -> Result<Self, ModelError> {
        let stable_id = stable_id.into();
        validate_required_field(&stable_id)?;
        Ok(Self {
            platform,
            stable_id,
            signer_id: None,
            include_helpers: false,
        })
    }

    pub fn with_signer_id(mut self, signer_id: impl Into<String>) -> Result<Self, ModelError> {
        self.signer_id = Some(validate_optional_field(signer_id.into())?);
        Ok(self)
    }

    #[must_use]
    pub fn include_helpers(mut self, include_helpers: bool) -> Self {
        self.include_helpers = include_helpers;
        self
    }

    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    #[must_use]
    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }

    #[must_use]
    pub fn signer_id(&self) -> Option<&str> {
        self.signer_id.as_deref()
    }

    #[must_use]
    pub const fn includes_helpers(&self) -> bool {
        self.include_helpers
    }

    #[must_use]
    pub fn matches(&self, identity: &AppIdentity) -> bool {
        if self.platform != identity.platform {
            return false;
        }

        let stable_id_matches = self.stable_id == identity.stable_id
            || (self.include_helpers
                && (identity.parent_stable_id.as_deref() == Some(self.stable_id.as_str())
                    || identity.helper_group_id.as_deref() == Some(self.stable_id.as_str())));
        if !stable_id_matches {
            return false;
        }

        self.signer_id
            .as_deref()
            .is_none_or(|required| identity.signer_id.as_deref() == Some(required))
    }
}

fn validate_required_field(value: &str) -> Result<(), ModelError> {
    if value.is_empty() || value.trim() != value {
        return Err(ModelError::EmptyAppStableId);
    }
    validate_field_length(value)
}

fn validate_optional_field(value: String) -> Result<String, ModelError> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(ModelError::InvalidAppIdentityField);
    }
    validate_field_length(&value)?;
    Ok(value)
}

fn validate_field_length(value: &str) -> Result<(), ModelError> {
    if value.chars().any(char::is_control) {
        return Err(ModelError::InvalidAppIdentityField);
    }
    if value.len() > MAX_IDENTITY_FIELD_LENGTH {
        return Err(ModelError::AppIdentityFieldTooLong);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn must_identity(result: Result<AppIdentity, ModelError>) -> AppIdentity {
        match result {
            Ok(identity) => identity,
            Err(error) => panic!("测试身份创建失败: {error}"),
        }
    }

    fn must_matcher(result: Result<AppMatcher, ModelError>) -> AppMatcher {
        match result {
            Ok(matcher) => matcher,
            Err(error) => panic!("测试匹配器创建失败: {error}"),
        }
    }

    #[test]
    fn helper_matching_requires_explicit_opt_in() {
        let identity = must_identity(
            AppIdentity::new(Platform::MacOs, "com.example.helper")
                .and_then(|value| value.with_parent_stable_id("com.example.app")),
        );
        let strict = must_matcher(AppMatcher::new(Platform::MacOs, "com.example.app"));
        let grouped = strict.clone().include_helpers(true);

        assert!(!strict.matches(&identity));
        assert!(grouped.matches(&identity));
    }

    #[test]
    fn signer_constraint_prevents_identity_spoofing() {
        let identity = must_identity(
            AppIdentity::new(Platform::MacOs, "com.example.app")
                .and_then(|value| value.with_signer_id("TEAM-A")),
        );
        let matcher = must_matcher(
            AppMatcher::new(Platform::MacOs, "com.example.app")
                .and_then(|value| value.with_signer_id("TEAM-B")),
        );

        assert!(!matcher.matches(&identity));
    }

    #[test]
    fn unknown_identity_is_explicit_and_platform_scoped() {
        let identity = AppIdentity::unknown(Platform::Windows);

        assert_eq!(identity.stable_id(), UNKNOWN_APP_STABLE_ID);
        assert_eq!(identity.platform(), Platform::Windows);
        assert_eq!(identity.signer_id(), None);
    }
}
