use nonproxy_learning::{
    AppLearningSubject, BrowserContextId, LearningCandidate, LearningCandidateKind,
    LearningObservation, LearningObservationKind, LearningResourceType, LearningSession,
    LearningSessionId, LearningSessionKind, LearningSessionState, LearningSubject, ObservationId,
};
use nonproxy_model::{AppIdentity, DomainName, Platform};
use nonproxy_proto::{
    common::v1::{AppIdentity as ProtoAppIdentity, Platform as ProtoPlatform},
    control::v1::{
        self as control_proto, LearningCandidate as ProtoCandidate,
        LearningObservationKind as ProtoObservationKind, LearningResourceType as ProtoResourceType,
        LearningSessionKind as ProtoSessionKind, LearningSessionState as ProtoSessionState,
        learning_session_summary,
    },
    events::v1::LearningCandidateKind as ProtoCandidateKind,
};

use crate::{GatewayError, clock::timestamp_from_unix_ms};

pub fn start_subject(
    request: &control_proto::StartLearningSessionRequest,
) -> Result<(LearningSubject, Option<BrowserContextId>), GatewayError> {
    let kind = ProtoSessionKind::try_from(request.kind)
        .map_err(|_| GatewayError::InvalidRequest("学习会话 kind 无效"))?;
    let subject = match (kind, request.subject.as_ref()) {
        (
            ProtoSessionKind::App,
            Some(control_proto::start_learning_session_request::Subject::App(app)),
        ) => LearningSubject::App(AppLearningSubject::from_identity(&app_from_proto(app)?)),
        (
            ProtoSessionKind::Site,
            Some(control_proto::start_learning_session_request::Subject::NormalizedSite(site)),
        ) => LearningSubject::Site(normalized_domain(site)?),
        _ => return Err(GatewayError::InvalidRequest("学习会话目标与 kind 不匹配")),
    };
    let context = optional_identifier(&request.browser_context_id, BrowserContextId::new)?;
    Ok((subject, context))
}

pub fn observation_from_proto(
    request: &control_proto::RecordLearningObservationRequest,
) -> Result<LearningObservation, GatewayError> {
    let kind = match ProtoObservationKind::try_from(request.kind)
        .map_err(|_| GatewayError::InvalidRequest("学习观测 kind 无效"))?
    {
        ProtoObservationKind::MainFrame => LearningObservationKind::MainFrame,
        ProtoObservationKind::Subresource => LearningObservationKind::Subresource,
        ProtoObservationKind::Redirect => LearningObservationKind::Redirect,
        ProtoObservationKind::Unspecified => {
            return Err(GatewayError::InvalidRequest("学习观测 kind 不能为空"));
        }
    };
    let resource_type = resource_type_from_proto(request.resource_type)?;
    Ok(LearningObservation::new(
        LearningSessionId::new(request.session_id.clone())?,
        ObservationId::new(request.observation_id.clone())?,
        optional_identifier(&request.browser_context_id, BrowserContextId::new)?,
        kind,
        normalized_domain(&request.normalized_domain)?,
        if request.initiator_domain.is_empty() {
            None
        } else {
            Some(normalized_domain(&request.initiator_domain)?)
        },
        resource_type,
        false,
    ))
}

pub fn session_id(value: &str) -> Result<LearningSessionId, GatewayError> {
    LearningSessionId::new(value.to_owned()).map_err(GatewayError::from)
}

pub fn session_to_proto(
    session: &LearningSession,
) -> Result<control_proto::LearningSessionSummary, GatewayError> {
    let subject = match session.subject() {
        LearningSubject::App(app) => learning_session_summary::Subject::App(app_to_proto(app)),
        LearningSubject::Site(site) => {
            learning_session_summary::Subject::NormalizedSite(site.as_ascii().to_owned())
        }
    };
    Ok(control_proto::LearningSessionSummary {
        session_id: session.id().as_str().to_owned(),
        kind: session_kind_to_proto(session.subject().kind()) as i32,
        state: session_state_to_proto(session.state()) as i32,
        browser_context_id: session
            .browser_context_id()
            .map_or_else(String::new, |value| value.as_str().to_owned()),
        started_at: Some(timestamp_from_unix_ms(session.started_at_unix_ms())?),
        expires_at: Some(timestamp_from_unix_ms(session.expires_at_unix_ms())?),
        stopped_at: session
            .stopped_at_unix_ms()
            .map(timestamp_from_unix_ms)
            .transpose()?,
        subject: Some(subject),
    })
}

pub fn candidate_to_proto(candidate: &LearningCandidate) -> Result<ProtoCandidate, GatewayError> {
    Ok(ProtoCandidate {
        normalized_domain: candidate.domain().as_ascii().to_owned(),
        registrable_domain: candidate
            .domain()
            .registrable()
            .unwrap_or_default()
            .to_owned(),
        kind: candidate_kind_to_proto(candidate.kind()) as i32,
        confidence: f32::from(candidate.confidence_millis()) / 1_000.0,
        requires_confirmation: candidate.requires_confirmation(),
        evidence_count: candidate.evidence_count(),
        first_seen_at: Some(timestamp_from_unix_ms(candidate.first_seen_at_unix_ms())?),
        last_seen_at: Some(timestamp_from_unix_ms(candidate.last_seen_at_unix_ms())?),
        main_frame_count: candidate.main_frame_count(),
        subresource_count: candidate.subresource_count(),
        redirect_count: candidate.redirect_count(),
    })
}

#[must_use]
pub const fn candidate_kind_to_proto(kind: LearningCandidateKind) -> ProtoCandidateKind {
    match kind {
        LearningCandidateKind::RequiredFirstParty => ProtoCandidateKind::RequiredFirstParty,
        LearningCandidateKind::LikelyApi => ProtoCandidateKind::LikelyApi,
        LearningCandidateKind::LikelyAuth => ProtoCandidateKind::LikelyAuth,
        LearningCandidateKind::LikelyCdn => ProtoCandidateKind::LikelyCdn,
        LearningCandidateKind::ThirdParty => ProtoCandidateKind::ThirdParty,
        LearningCandidateKind::Unknown => ProtoCandidateKind::Unknown,
    }
}

pub fn duration_ms(duration: Option<&prost_types::Duration>) -> Result<u64, GatewayError> {
    let Some(duration) = duration else {
        return Ok(nonproxy_learning::DEFAULT_LEARNING_DURATION_MS);
    };
    let value = std::time::Duration::try_from(*duration)
        .map_err(|_| GatewayError::InvalidRequest("学习时长无效"))?;
    u64::try_from(value.as_millis()).map_err(|_| GatewayError::InvalidRequest("学习时长超出范围"))
}

fn normalized_domain(value: &str) -> Result<DomainName, GatewayError> {
    let domain = DomainName::normalize(value)?;
    if domain.as_ascii() != value {
        return Err(GatewayError::InvalidRequest("域名必须预先规范化"));
    }
    Ok(domain)
}

fn optional_identifier<T>(
    value: &str,
    factory: impl FnOnce(String) -> Result<T, nonproxy_learning::LearningError>,
) -> Result<Option<T>, GatewayError> {
    if value.is_empty() {
        Ok(None)
    } else {
        factory(value.to_owned())
            .map(Some)
            .map_err(GatewayError::from)
    }
}

fn app_from_proto(value: &ProtoAppIdentity) -> Result<AppIdentity, GatewayError> {
    let platform = match ProtoPlatform::try_from(value.platform)
        .map_err(|_| GatewayError::InvalidRequest("应用平台无效"))?
    {
        ProtoPlatform::Macos => Platform::MacOs,
        ProtoPlatform::Windows => Platform::Windows,
        ProtoPlatform::Unspecified => {
            return Err(GatewayError::InvalidRequest("应用平台不能为空"));
        }
    };
    let mut identity = AppIdentity::new(platform, value.stable_id.clone())?;
    if !value.signer_id.is_empty() {
        identity = identity.with_signer_id(value.signer_id.clone())?;
    }
    Ok(identity)
}

fn app_to_proto(value: &AppLearningSubject) -> ProtoAppIdentity {
    ProtoAppIdentity {
        platform: match value.platform() {
            Platform::MacOs => ProtoPlatform::Macos as i32,
            Platform::Windows => ProtoPlatform::Windows as i32,
        },
        stable_id: value.stable_id().to_owned(),
        signer_id: value.signer_id().unwrap_or_default().to_owned(),
        ..Default::default()
    }
}

fn resource_type_from_proto(value: i32) -> Result<LearningResourceType, GatewayError> {
    let value = ProtoResourceType::try_from(value)
        .map_err(|_| GatewayError::InvalidRequest("学习资源类型无效"))?;
    match value {
        ProtoResourceType::MainFrame => Ok(LearningResourceType::MainFrame),
        ProtoResourceType::SubFrame => Ok(LearningResourceType::SubFrame),
        ProtoResourceType::Script => Ok(LearningResourceType::Script),
        ProtoResourceType::StyleSheet => Ok(LearningResourceType::StyleSheet),
        ProtoResourceType::Image => Ok(LearningResourceType::Image),
        ProtoResourceType::Font => Ok(LearningResourceType::Font),
        ProtoResourceType::Media => Ok(LearningResourceType::Media),
        ProtoResourceType::XmlHttpRequest => Ok(LearningResourceType::XmlHttpRequest),
        ProtoResourceType::Fetch => Ok(LearningResourceType::Fetch),
        ProtoResourceType::WebSocket => Ok(LearningResourceType::WebSocket),
        ProtoResourceType::Other => Ok(LearningResourceType::Other),
        ProtoResourceType::Unspecified => Err(GatewayError::InvalidRequest("学习资源类型不能为空")),
    }
}

const fn session_kind_to_proto(kind: LearningSessionKind) -> ProtoSessionKind {
    match kind {
        LearningSessionKind::App => ProtoSessionKind::App,
        LearningSessionKind::Site => ProtoSessionKind::Site,
    }
}

const fn session_state_to_proto(state: LearningSessionState) -> ProtoSessionState {
    match state {
        LearningSessionState::Active => ProtoSessionState::Active,
        LearningSessionState::Stopped => ProtoSessionState::Stopped,
        LearningSessionState::Expired => ProtoSessionState::Expired,
    }
}
