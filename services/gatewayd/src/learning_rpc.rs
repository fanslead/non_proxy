use nonproxy_proto::control::v1 as control_proto;
use tonic::Status;

use crate::{Gateway, control_mapping, control_rpc_helpers::request_status, learning_contract};

pub async fn start(
    gateway: &Gateway,
    request: control_proto::StartLearningSessionRequest,
) -> Result<control_proto::StartLearningSessionResponse, Status> {
    let (subject, context) = learning_contract::start_subject(&request).map_err(request_status)?;
    let duration =
        learning_contract::duration_ms(request.duration.as_ref()).map_err(request_status)?;
    match gateway.start_learning(subject, context, duration).await {
        Ok(session) => Ok(control_proto::StartLearningSessionResponse {
            session_id: session.id().as_str().to_owned(),
            expires_at: Some(
                crate::clock::timestamp_from_unix_ms(session.expires_at_unix_ms())
                    .map_err(crate::control_rpc_helpers::internal_status)?,
            ),
            error: None,
        }),
        Err(error) => Ok(control_proto::StartLearningSessionResponse {
            error: Some(control_mapping::error_detail(&error)),
            ..Default::default()
        }),
    }
}

pub async fn record(
    gateway: &Gateway,
    request: control_proto::RecordLearningObservationRequest,
) -> Result<control_proto::RecordLearningObservationResponse, Status> {
    let observation =
        learning_contract::observation_from_proto(&request).map_err(request_status)?;
    match gateway.record_learning_observation(observation).await {
        Ok(result) => Ok(control_proto::RecordLearningObservationResponse {
            candidate: Some(
                learning_contract::candidate_to_proto(result.candidate())
                    .map_err(crate::control_rpc_helpers::internal_status)?,
            ),
            duplicate: result.duplicate(),
            error: None,
        }),
        Err(error) => Ok(control_proto::RecordLearningObservationResponse {
            error: Some(control_mapping::error_detail(&error)),
            ..Default::default()
        }),
    }
}

pub async fn list(
    gateway: &Gateway,
    request: control_proto::ListLearningCandidatesRequest,
) -> Result<control_proto::ListLearningCandidatesResponse, Status> {
    let session_id = learning_contract::session_id(&request.session_id).map_err(request_status)?;
    match gateway.learning_candidates(session_id).await {
        Ok((session, candidates)) => Ok(control_proto::ListLearningCandidatesResponse {
            session: Some(
                learning_contract::session_to_proto(&session)
                    .map_err(crate::control_rpc_helpers::internal_status)?,
            ),
            candidates: candidates
                .iter()
                .map(learning_contract::candidate_to_proto)
                .collect::<Result<Vec<_>, _>>()
                .map_err(crate::control_rpc_helpers::internal_status)?,
            error: None,
        }),
        Err(error) => Ok(control_proto::ListLearningCandidatesResponse {
            error: Some(control_mapping::error_detail(&error)),
            ..Default::default()
        }),
    }
}

pub async fn stop(
    gateway: &Gateway,
    request: control_proto::StopLearningSessionRequest,
) -> Result<control_proto::StopLearningSessionResponse, Status> {
    let session_id = learning_contract::session_id(&request.session_id).map_err(request_status)?;
    match gateway.stop_learning(session_id).await {
        Ok(stopped) => Ok(control_proto::StopLearningSessionResponse {
            candidate_count: stopped.candidate_count(),
            error: None,
            session: Some(
                learning_contract::session_to_proto(stopped.session())
                    .map_err(crate::control_rpc_helpers::internal_status)?,
            ),
        }),
        Err(error) => Ok(control_proto::StopLearningSessionResponse {
            error: Some(control_mapping::error_detail(&error)),
            ..Default::default()
        }),
    }
}

pub async fn confirm(
    gateway: &Gateway,
    request: control_proto::ConfirmLearningCandidatesRequest,
) -> Result<control_proto::ConfirmLearningCandidatesResponse, Status> {
    let session_id = learning_contract::session_id(&request.session_id).map_err(request_status)?;
    let confirmation_id =
        learning_contract::confirmation_id(&request.confirmation_id).map_err(request_status)?;
    let selected_domains =
        learning_contract::selected_domains(&request.selected_domains).map_err(request_status)?;
    match gateway
        .confirm_learning_candidates(session_id, confirmation_id, selected_domains)
        .await
    {
        Ok(confirmed) => {
            let record = confirmed.snapshot();
            let metadata = control_mapping::snapshot_metadata(
                record.artifact(),
                control_mapping::snapshot_state(record.status()),
            )
            .map_err(crate::control_rpc_helpers::internal_status)?;
            if confirmed.snapshot_staged() {
                crate::control_rpc_helpers::publish_snapshot_event(gateway, metadata.clone())?;
                let _ = gateway.publish_runtime_events().await;
            }
            Ok(control_proto::ConfirmLearningCandidatesResponse {
                policies: confirmed
                    .receipt()
                    .policies()
                    .iter()
                    .map(|value| control_proto::ConfirmedLearningPolicy {
                        normalized_domain: value.domain().as_ascii().to_owned(),
                        policy_id: value.policy_id().as_str().to_owned(),
                    })
                    .collect(),
                snapshot: Some(metadata),
                replayed: confirmed.replayed(),
                conflicts: Vec::new(),
                error: None,
            })
        }
        Err(error) => Ok(control_proto::ConfirmLearningCandidatesResponse {
            conflicts: match &error {
                crate::GatewayError::Compile(compile) => {
                    control_mapping::compile_conflicts(compile)
                }
                _ => Vec::new(),
            },
            error: Some(control_mapping::error_detail(&error)),
            ..Default::default()
        }),
    }
}
