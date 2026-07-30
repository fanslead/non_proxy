use nonproxy_model::DomainName;

use crate::{BrowserContextId, LearningSessionId, ObservationId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningObservationKind {
    MainFrame,
    Subresource,
    Redirect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningResourceType {
    MainFrame,
    SubFrame,
    Script,
    StyleSheet,
    Image,
    Font,
    Media,
    XmlHttpRequest,
    Fetch,
    WebSocket,
    Other,
}

impl LearningResourceType {
    #[must_use]
    pub const fn is_api(self) -> bool {
        matches!(self, Self::XmlHttpRequest | Self::Fetch | Self::WebSocket)
    }

    #[must_use]
    pub const fn is_static_asset(self) -> bool {
        matches!(
            self,
            Self::Script | Self::StyleSheet | Self::Image | Self::Font | Self::Media
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningObservation {
    session_id: LearningSessionId,
    observation_id: ObservationId,
    browser_context_id: Option<BrowserContextId>,
    kind: LearningObservationKind,
    domain: DomainName,
    initiator_domain: Option<DomainName>,
    resource_type: LearningResourceType,
    cname_correlated: bool,
}

impl LearningObservation {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: LearningSessionId,
        observation_id: ObservationId,
        browser_context_id: Option<BrowserContextId>,
        kind: LearningObservationKind,
        domain: DomainName,
        initiator_domain: Option<DomainName>,
        resource_type: LearningResourceType,
        cname_correlated: bool,
    ) -> Self {
        Self {
            session_id,
            observation_id,
            browser_context_id,
            kind,
            domain,
            initiator_domain,
            resource_type,
            cname_correlated,
        }
    }

    #[must_use]
    pub const fn session_id(&self) -> &LearningSessionId {
        &self.session_id
    }

    #[must_use]
    pub const fn observation_id(&self) -> &ObservationId {
        &self.observation_id
    }

    #[must_use]
    pub const fn browser_context_id(&self) -> Option<&BrowserContextId> {
        self.browser_context_id.as_ref()
    }

    #[must_use]
    pub const fn kind(&self) -> LearningObservationKind {
        self.kind
    }

    #[must_use]
    pub const fn domain(&self) -> &DomainName {
        &self.domain
    }

    #[must_use]
    pub const fn initiator_domain(&self) -> Option<&DomainName> {
        self.initiator_domain.as_ref()
    }

    #[must_use]
    pub const fn resource_type(&self) -> LearningResourceType {
        self.resource_type
    }

    #[must_use]
    pub const fn cname_correlated(&self) -> bool {
        self.cname_correlated
    }
}
