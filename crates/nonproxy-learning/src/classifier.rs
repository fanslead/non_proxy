use crate::{
    LearningCandidate, LearningCandidateKind, LearningObservation, LearningObservationKind,
    LearningSession,
};

const AUTO_ELIGIBLE_CONFIDENCE: u16 = 900;

#[must_use]
pub fn classify(
    session: &LearningSession,
    observation: &LearningObservation,
    previous: Option<&LearningCandidate>,
    now_unix_ms: u64,
) -> LearningCandidate {
    let evidence_count = previous
        .map_or(1, LearningCandidate::evidence_count)
        .saturating_add(u32::from(previous.is_some()));
    let target_site = session.subject().site();
    let same_party = target_site.is_some_and(|target| {
        target == observation.domain()
            || (target.registrable().is_some()
                && target.registrable() == observation.domain().registrable())
    });
    let (kind, base_confidence) = candidate_kind(observation, same_party);
    let frequency_bonus = u16::try_from(evidence_count.saturating_sub(1).min(6)).unwrap_or(6) * 25;
    let cname_bonus = u16::from(observation.cname_correlated()) * 40;
    let confidence_millis = base_confidence
        .saturating_add(frequency_bonus)
        .saturating_add(cname_bonus)
        .min(1_000);
    let requires_confirmation = kind != LearningCandidateKind::RequiredFirstParty
        || confidence_millis < AUTO_ELIGIBLE_CONFIDENCE;
    let first_seen = previous.map_or(now_unix_ms, LearningCandidate::first_seen_at_unix_ms);
    let (main_frame_count, subresource_count, redirect_count) =
        increment_counts(previous, observation.kind());

    LearningCandidate::new(
        observation.domain().clone(),
        kind,
        confidence_millis,
        requires_confirmation,
        evidence_count,
        first_seen,
        now_unix_ms,
        main_frame_count,
        subresource_count,
        redirect_count,
    )
}

fn candidate_kind(
    observation: &LearningObservation,
    same_party: bool,
) -> (LearningCandidateKind, u16) {
    if same_party {
        let base = if observation.kind() == LearningObservationKind::MainFrame {
            1_000
        } else {
            850
        };
        return (LearningCandidateKind::RequiredFirstParty, base);
    }

    let domain = observation.domain().as_ascii();
    if contains_label(
        domain,
        &["auth", "login", "oauth", "account", "sso", "identity"],
    ) {
        return (LearningCandidateKind::LikelyAuth, 760);
    }
    if observation.resource_type().is_api()
        || contains_label(domain, &["api", "graphql", "socket", "ws"])
    {
        return (LearningCandidateKind::LikelyApi, 720);
    }
    if observation.resource_type().is_static_asset()
        && contains_label(
            domain,
            &["cdn", "static", "asset", "media", "content", "image", "img"],
        )
    {
        return (LearningCandidateKind::LikelyCdn, 680);
    }
    if observation.initiator_domain().is_some() {
        return (LearningCandidateKind::ThirdParty, 450);
    }
    (LearningCandidateKind::Unknown, 300)
}

fn contains_label(domain: &str, labels: &[&str]) -> bool {
    domain.split('.').any(|label| labels.contains(&label))
}

fn increment_counts(
    previous: Option<&LearningCandidate>,
    kind: LearningObservationKind,
) -> (u32, u32, u32) {
    let mut main = previous.map_or(0, LearningCandidate::main_frame_count);
    let mut subresource = previous.map_or(0, LearningCandidate::subresource_count);
    let mut redirect = previous.map_or(0, LearningCandidate::redirect_count);
    match kind {
        LearningObservationKind::MainFrame => main = main.saturating_add(1),
        LearningObservationKind::Subresource => {
            subresource = subresource.saturating_add(1);
        }
        LearningObservationKind::Redirect => redirect = redirect.saturating_add(1),
    }
    (main, subresource, redirect)
}

#[cfg(test)]
mod tests {
    use nonproxy_model::DomainName;

    use crate::{
        BrowserContextId, LearningObservation, LearningObservationKind, LearningResourceType,
        LearningSession, LearningSessionId, LearningSubject, ObservationId,
    };

    use super::*;

    #[test]
    fn same_site_dependency_becomes_auto_eligible_only_after_repeated_evidence() {
        let session = session();
        let observation = observation("api.example.com", LearningResourceType::Fetch);
        let first = classify(&session, &observation, None, 2_000);
        let second = classify(&session, &observation, Some(&first), 2_100);
        let third = classify(&session, &observation, Some(&second), 2_200);

        assert_eq!(third.kind(), LearningCandidateKind::RequiredFirstParty);
        assert_eq!(third.confidence_millis(), 900);
        assert!(!third.requires_confirmation());
    }

    #[test]
    fn cross_site_auth_never_becomes_implicitly_eligible() {
        let session = session();
        let observation = observation_with_kind(
            "login.identity.test",
            LearningResourceType::MainFrame,
            LearningObservationKind::MainFrame,
        );
        let candidate = classify(&session, &observation, None, 2_000);

        assert_eq!(candidate.kind(), LearningCandidateKind::LikelyAuth);
        assert!(candidate.requires_confirmation());
    }

    fn session() -> LearningSession {
        let id = LearningSessionId::new("learning-test");
        let context = BrowserContextId::new("tab-test");
        let site = DomainName::normalize("www.example.com");
        let (Ok(id), Ok(context), Ok(site)) = (id, context, site) else {
            panic!("学习测试输入无效");
        };
        match LearningSession::start(
            id,
            LearningSubject::Site(site),
            Some(context),
            1_000,
            60_000,
        ) {
            Ok(value) => value,
            Err(error) => panic!("学习测试会话创建失败: {error}"),
        }
    }

    fn observation(domain: &str, resource_type: LearningResourceType) -> LearningObservation {
        observation_with_kind(domain, resource_type, LearningObservationKind::Subresource)
    }

    fn observation_with_kind(
        domain: &str,
        resource_type: LearningResourceType,
        kind: LearningObservationKind,
    ) -> LearningObservation {
        let session_id = LearningSessionId::new("learning-test");
        let observation_id = ObservationId::new(format!("observation-{domain}"));
        let context = BrowserContextId::new("tab-test");
        let domain = DomainName::normalize(domain);
        let initiator = DomainName::normalize("www.example.com");
        let (Ok(session_id), Ok(observation_id), Ok(context), Ok(domain), Ok(initiator)) =
            (session_id, observation_id, context, domain, initiator)
        else {
            panic!("学习测试观测创建失败");
        };
        LearningObservation::new(
            session_id,
            observation_id,
            Some(context),
            kind,
            domain,
            Some(initiator),
            resource_type,
            false,
        )
    }
}
