use nonproxy_proto::{
    common::v1::{ComponentKind, Severity},
    control::v1::{self as control_proto, PolicyMutationResult},
    events::v1::EventEnvelope,
    policy::v1::PolicySnapshotMetadata,
};
use tonic::Status;

use crate::{Gateway, GatewayError, control_mapping};

pub fn empty_mutation() -> PolicyMutationResult {
    PolicyMutationResult {
        policy: None,
        snapshot: None,
        conflicts: Vec::new(),
        error: None,
    }
}

pub fn mutation_error(error: &GatewayError) -> PolicyMutationResult {
    let conflicts = match error {
        GatewayError::Compile(compile) => control_mapping::compile_conflicts(compile),
        _ => Vec::new(),
    };
    PolicyMutationResult {
        policy: None,
        snapshot: None,
        conflicts,
        error: Some(control_mapping::error_detail(error)),
    }
}

pub fn publish_snapshot_event(
    gateway: &Gateway,
    metadata: PolicySnapshotMetadata,
) -> Result<(), Status> {
    let event = EventEnvelope {
        component: ComponentKind::Gateway as i32,
        severity: Severity::Info as i32,
        snapshot_version: metadata.snapshot_version,
        payload: Some(
            nonproxy_proto::events::v1::event_envelope::Payload::SnapshotStateChanged(
                nonproxy_proto::events::v1::SnapshotStateChanged {
                    snapshot: Some(metadata),
                    conflicts: Vec::new(),
                },
            ),
        ),
        ..Default::default()
    };
    gateway.events().publish(event).map_err(internal_status)?;
    Ok(())
}

pub fn minimum_severity(value: i32) -> Result<i32, Status> {
    let severity =
        Severity::try_from(value).map_err(|_| Status::invalid_argument("minimum_severity 无效"))?;
    Ok(match severity {
        Severity::Unspecified => Severity::Debug as i32,
        _ => severity as i32,
    })
}

#[must_use]
pub fn event_meets_minimum(event: &EventEnvelope, minimum: i32) -> bool {
    matches!(
        Severity::try_from(event.severity),
        Ok(severity)
            if severity != Severity::Unspecified && event.severity >= minimum
    )
}

pub fn event_response(
    event: EventEnvelope,
) -> Result<control_proto::SubscribeEventsResponse, Status> {
    Ok(control_proto::SubscribeEventsResponse { event: Some(event) })
}

pub fn internal_status(error: GatewayError) -> Status {
    Status::internal(format!("{}: {}", error.code(), error))
}

pub fn request_status(error: GatewayError) -> Status {
    Status::invalid_argument(format!("{}: {}", error.code(), error))
}

#[cfg(test)]
mod tests {
    use nonproxy_proto::{common::v1::Severity, events::v1::EventEnvelope};

    use super::{event_meets_minimum, minimum_severity};

    #[test]
    fn omitted_minimum_severity_includes_debug_events() {
        let minimum = minimum_severity(Severity::Unspecified as i32);

        assert!(matches!(minimum, Ok(value) if value == Severity::Debug as i32));
    }

    #[test]
    fn minimum_severity_filters_less_severe_events() {
        let info = EventEnvelope {
            severity: Severity::Info as i32,
            ..Default::default()
        };
        let error = EventEnvelope {
            severity: Severity::Error as i32,
            ..Default::default()
        };

        assert!(!event_meets_minimum(&info, Severity::Warning as i32));
        assert!(event_meets_minimum(&error, Severity::Warning as i32));
    }

    #[test]
    fn unknown_minimum_severity_is_rejected() {
        assert!(minimum_severity(99).is_err());
    }
}
