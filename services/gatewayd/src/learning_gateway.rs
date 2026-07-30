use nonproxy_learning::{
    BrowserContextId, LearningObservation, LearningSession, LearningSessionId, LearningSubject,
};
use nonproxy_proto::{
    common::v1::{ComponentKind, Severity},
    events::v1::{EventEnvelope, LearningCandidateUpdated, event_envelope},
};
use nonproxy_storage::{LearningObservationResult, StoppedLearning};

use crate::{Gateway, GatewayError, clock::unix_time_ms, learning_contract};

impl Gateway {
    pub async fn start_learning(
        &self,
        subject: LearningSubject,
        browser_context_id: Option<BrowserContextId>,
        duration_ms: u64,
    ) -> Result<LearningSession, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let session = LearningSession::start(
            new_session_id()?,
            subject,
            browser_context_id,
            now,
            duration_ms,
        )?;
        let stored = session.clone();
        self.database
            .run(move |database| {
                database.learning().start(&stored)?;
                Ok(())
            })
            .await?;
        Ok(session)
    }

    pub async fn learning_candidates(
        &self,
        session_id: LearningSessionId,
    ) -> Result<(LearningSession, Vec<nonproxy_learning::LearningCandidate>), GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| Ok(database.learning().list_candidates(&session_id, now)?))
            .await
    }

    pub async fn record_learning_observation(
        &self,
        observation: LearningObservation,
    ) -> Result<LearningObservationResult, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        let session_id = observation.session_id().clone();
        let result = self
            .database
            .run(move |database| Ok(database.learning().record_observation(&observation, now)?))
            .await?;
        if !result.duplicate() {
            publish_candidate(self, &session_id, &result)?;
        }
        Ok(result)
    }

    pub async fn stop_learning(
        &self,
        session_id: LearningSessionId,
    ) -> Result<StoppedLearning, GatewayError> {
        let _operation = self.mutation_gate.lock().await;
        let now = unix_time_ms()?;
        self.database
            .run(move |database| Ok(database.learning().stop(&session_id, now)?))
            .await
    }
}

fn new_session_id() -> Result<LearningSessionId, GatewayError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| GatewayError::Random(error.to_string()))?;
    let suffix = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    LearningSessionId::new(format!("learning-{suffix}")).map_err(GatewayError::from)
}

fn publish_candidate(
    gateway: &Gateway,
    session_id: &LearningSessionId,
    result: &LearningObservationResult,
) -> Result<(), GatewayError> {
    let candidate = result.candidate();
    let payload = LearningCandidateUpdated {
        session_id: session_id.as_str().to_owned(),
        normalized_domain: candidate.domain().as_ascii().to_owned(),
        kind: learning_contract::candidate_kind_to_proto(candidate.kind()) as i32,
        confidence: f32::from(candidate.confidence_millis()) / 1_000.0,
        requires_confirmation: candidate.requires_confirmation(),
    };
    gateway.events().publish(EventEnvelope {
        component: ComponentKind::Gateway as i32,
        severity: Severity::Info as i32,
        payload: Some(event_envelope::Payload::LearningCandidateUpdated(payload)),
        ..Default::default()
    })?;
    Ok(())
}
