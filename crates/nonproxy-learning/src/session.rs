use nonproxy_model::{AppIdentity, DomainName, Platform};

use crate::{BrowserContextId, LearningError, LearningSessionId};

pub const DEFAULT_LEARNING_DURATION_MS: u64 = 60_000;
pub const MIN_LEARNING_DURATION_MS: u64 = 5_000;
pub const MAX_LEARNING_DURATION_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningSessionKind {
    App,
    Site,
}

impl LearningSessionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Site => "site",
        }
    }

    pub fn parse(value: &str) -> Result<Self, LearningError> {
        match value {
            "app" => Ok(Self::App),
            "site" => Ok(Self::Site),
            _ => Err(LearningError::InvalidIdentifier),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningSessionState {
    Active,
    Stopped,
    Expired,
}

impl LearningSessionState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Stopped => "stopped",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Result<Self, LearningError> {
        match value {
            "active" => Ok(Self::Active),
            "stopped" => Ok(Self::Stopped),
            "expired" => Ok(Self::Expired),
            _ => Err(LearningError::InvalidIdentifier),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppLearningSubject {
    platform: Platform,
    stable_id: String,
    signer_id: Option<String>,
}

impl AppLearningSubject {
    #[must_use]
    pub fn from_identity(identity: &AppIdentity) -> Self {
        Self {
            platform: identity.platform(),
            stable_id: identity.stable_id().to_owned(),
            signer_id: identity.signer_id().map(str::to_owned),
        }
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LearningSubject {
    App(AppLearningSubject),
    Site(DomainName),
}

impl LearningSubject {
    #[must_use]
    pub const fn kind(&self) -> LearningSessionKind {
        match self {
            Self::App(_) => LearningSessionKind::App,
            Self::Site(_) => LearningSessionKind::Site,
        }
    }

    #[must_use]
    pub fn target(&self) -> &str {
        match self {
            Self::App(app) => app.stable_id(),
            Self::Site(site) => site.as_ascii(),
        }
    }

    #[must_use]
    pub fn site(&self) -> Option<&DomainName> {
        match self {
            Self::Site(site) => Some(site),
            Self::App(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningSession {
    id: LearningSessionId,
    subject: LearningSubject,
    browser_context_id: Option<BrowserContextId>,
    state: LearningSessionState,
    started_at_unix_ms: u64,
    expires_at_unix_ms: u64,
    stopped_at_unix_ms: Option<u64>,
}

impl LearningSession {
    pub fn start(
        id: LearningSessionId,
        subject: LearningSubject,
        browser_context_id: Option<BrowserContextId>,
        started_at_unix_ms: u64,
        duration_ms: u64,
    ) -> Result<Self, LearningError> {
        if !(MIN_LEARNING_DURATION_MS..=MAX_LEARNING_DURATION_MS).contains(&duration_ms) {
            return Err(LearningError::InvalidDuration);
        }
        validate_context(&subject, browser_context_id.as_ref())?;
        let expires_at_unix_ms = started_at_unix_ms
            .checked_add(duration_ms)
            .ok_or(LearningError::InvalidTimeRange)?;
        Ok(Self {
            id,
            subject,
            browser_context_id,
            state: LearningSessionState::Active,
            started_at_unix_ms,
            expires_at_unix_ms,
            stopped_at_unix_ms: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: LearningSessionId,
        subject: LearningSubject,
        browser_context_id: Option<BrowserContextId>,
        state: LearningSessionState,
        started_at_unix_ms: u64,
        expires_at_unix_ms: u64,
        stopped_at_unix_ms: Option<u64>,
    ) -> Result<Self, LearningError> {
        validate_context(&subject, browser_context_id.as_ref())?;
        if expires_at_unix_ms <= started_at_unix_ms
            || stopped_at_unix_ms.is_some_and(|value| value < started_at_unix_ms)
            || matches!(
                (state, stopped_at_unix_ms),
                (LearningSessionState::Active, Some(_))
                    | (
                        LearningSessionState::Stopped | LearningSessionState::Expired,
                        None
                    )
            )
        {
            return Err(LearningError::InvalidTimeRange);
        }
        Ok(Self {
            id,
            subject,
            browser_context_id,
            state,
            started_at_unix_ms,
            expires_at_unix_ms,
            stopped_at_unix_ms,
        })
    }

    pub fn validate_observation(
        &self,
        context: Option<&BrowserContextId>,
        now_unix_ms: u64,
    ) -> Result<(), LearningError> {
        if self.state != LearningSessionState::Active || now_unix_ms >= self.expires_at_unix_ms {
            return Err(LearningError::SessionNotActive);
        }
        if self.browser_context_id.as_ref() != context {
            return Err(LearningError::BrowserContextMismatch);
        }
        Ok(())
    }

    #[must_use]
    pub const fn id(&self) -> &LearningSessionId {
        &self.id
    }

    #[must_use]
    pub const fn subject(&self) -> &LearningSubject {
        &self.subject
    }

    #[must_use]
    pub const fn browser_context_id(&self) -> Option<&BrowserContextId> {
        self.browser_context_id.as_ref()
    }

    #[must_use]
    pub const fn state(&self) -> LearningSessionState {
        self.state
    }

    #[must_use]
    pub const fn started_at_unix_ms(&self) -> u64 {
        self.started_at_unix_ms
    }

    #[must_use]
    pub const fn expires_at_unix_ms(&self) -> u64 {
        self.expires_at_unix_ms
    }

    #[must_use]
    pub const fn stopped_at_unix_ms(&self) -> Option<u64> {
        self.stopped_at_unix_ms
    }
}

fn validate_context(
    subject: &LearningSubject,
    browser_context_id: Option<&BrowserContextId>,
) -> Result<(), LearningError> {
    match (subject, browser_context_id) {
        (LearningSubject::Site(_), None) => Err(LearningError::BrowserContextRequired),
        (LearningSubject::App(_), Some(_)) => Err(LearningError::BrowserContextNotAllowed),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use nonproxy_model::{AppIdentity, DomainName, Platform};

    use super::*;

    #[test]
    fn site_and_app_sessions_enforce_distinct_context_boundaries() {
        let site_id = LearningSessionId::new("site-session");
        let site = DomainName::normalize("example.com");
        let app_id = LearningSessionId::new("app-session");
        let app = AppIdentity::new(Platform::MacOs, "com.example.app");
        let context = BrowserContextId::new("browser-context");
        let (Ok(site_id), Ok(site), Ok(app_id), Ok(app), Ok(context)) =
            (site_id, site, app_id, app, context)
        else {
            panic!("学习上下文测试输入无效");
        };

        assert!(matches!(
            LearningSession::start(
                site_id,
                LearningSubject::Site(site),
                None,
                1_000,
                DEFAULT_LEARNING_DURATION_MS
            ),
            Err(LearningError::BrowserContextRequired)
        ));
        assert!(matches!(
            LearningSession::start(
                app_id,
                LearningSubject::App(AppLearningSubject::from_identity(&app)),
                Some(context),
                1_000,
                DEFAULT_LEARNING_DURATION_MS
            ),
            Err(LearningError::BrowserContextNotAllowed)
        ));
    }

    #[test]
    fn restored_state_requires_a_matching_stop_timestamp() {
        let id = LearningSessionId::new("restored-session");
        let context = BrowserContextId::new("browser-context");
        let site = DomainName::normalize("example.com");
        let (Ok(id), Ok(context), Ok(site)) = (id, context, site) else {
            panic!("学习恢复测试输入无效");
        };

        assert!(matches!(
            LearningSession::restore(
                id,
                LearningSubject::Site(site),
                Some(context),
                LearningSessionState::Stopped,
                1_000,
                61_000,
                None
            ),
            Err(LearningError::InvalidTimeRange)
        ));
    }
}
