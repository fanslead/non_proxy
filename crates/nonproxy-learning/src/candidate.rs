use nonproxy_model::DomainName;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningCandidateKind {
    RequiredFirstParty,
    LikelyApi,
    LikelyAuth,
    LikelyCdn,
    ThirdParty,
    Unknown,
}

impl LearningCandidateKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequiredFirstParty => "required_first_party",
            Self::LikelyApi => "likely_api",
            Self::LikelyAuth => "likely_auth",
            Self::LikelyCdn => "likely_cdn",
            Self::ThirdParty => "third_party",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "required_first_party" => Some(Self::RequiredFirstParty),
            "likely_api" => Some(Self::LikelyApi),
            "likely_auth" => Some(Self::LikelyAuth),
            "likely_cdn" => Some(Self::LikelyCdn),
            "third_party" => Some(Self::ThirdParty),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearningCandidate {
    domain: DomainName,
    kind: LearningCandidateKind,
    confidence_millis: u16,
    requires_confirmation: bool,
    evidence_count: u32,
    first_seen_at_unix_ms: u64,
    last_seen_at_unix_ms: u64,
    main_frame_count: u32,
    subresource_count: u32,
    redirect_count: u32,
}

impl LearningCandidate {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        domain: DomainName,
        kind: LearningCandidateKind,
        confidence_millis: u16,
        requires_confirmation: bool,
        evidence_count: u32,
        first_seen_at_unix_ms: u64,
        last_seen_at_unix_ms: u64,
        main_frame_count: u32,
        subresource_count: u32,
        redirect_count: u32,
    ) -> Self {
        Self {
            domain,
            kind,
            confidence_millis,
            requires_confirmation,
            evidence_count,
            first_seen_at_unix_ms,
            last_seen_at_unix_ms,
            main_frame_count,
            subresource_count,
            redirect_count,
        }
    }

    #[must_use]
    pub const fn domain(&self) -> &DomainName {
        &self.domain
    }

    #[must_use]
    pub const fn kind(&self) -> LearningCandidateKind {
        self.kind
    }

    #[must_use]
    pub const fn confidence_millis(&self) -> u16 {
        self.confidence_millis
    }

    #[must_use]
    pub const fn requires_confirmation(&self) -> bool {
        self.requires_confirmation
    }

    #[must_use]
    pub const fn evidence_count(&self) -> u32 {
        self.evidence_count
    }

    #[must_use]
    pub const fn first_seen_at_unix_ms(&self) -> u64 {
        self.first_seen_at_unix_ms
    }

    #[must_use]
    pub const fn last_seen_at_unix_ms(&self) -> u64 {
        self.last_seen_at_unix_ms
    }

    #[must_use]
    pub const fn main_frame_count(&self) -> u32 {
        self.main_frame_count
    }

    #[must_use]
    pub const fn subresource_count(&self) -> u32 {
        self.subresource_count
    }

    #[must_use]
    pub const fn redirect_count(&self) -> u32 {
        self.redirect_count
    }
}
