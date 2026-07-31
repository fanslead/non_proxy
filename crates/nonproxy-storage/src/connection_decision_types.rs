use nonproxy_model::{AppIdentity, Decision, Destination, OutboundId, RouteAction, Transport};

use crate::{StorageError, connection_decision_codec::PersistedDecision};

const MAX_IDENTIFIER_LENGTH: usize = 512;
const MAX_SHORT_FIELD_LENGTH: usize = 128;
const MAX_DECISION_LATENCY_MICROS: u64 = 60_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceLevel {
    Decision,
    Path,
    Exit,
}

impl EvidenceLevel {
    pub(crate) const fn as_i64(self) -> i64 {
        match self {
            Self::Decision => 2,
            Self::Path => 3,
            Self::Exit => 4,
        }
    }

    pub(crate) fn parse(value: i64) -> Result<Self, StorageError> {
        match value {
            2 => Ok(Self::Decision),
            3 => Ok(Self::Path),
            4 => Ok(Self::Exit),
            _ => Err(StorageError::CorruptData {
                field: "connection_decision.evidence_level",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionEvidence {
    level: EvidenceLevel,
    interface_name: Option<String>,
    outbound_id: Option<OutboundId>,
    exit_probe_id: Option<String>,
}

impl DecisionEvidence {
    pub fn new(
        level: EvidenceLevel,
        interface_name: Option<String>,
        outbound_id: Option<OutboundId>,
        exit_probe_id: Option<String>,
    ) -> Result<Self, StorageError> {
        validate_optional_evidence_field(interface_name.as_deref())?;
        validate_optional_evidence_field(exit_probe_id.as_deref())?;
        let path_count = usize::from(interface_name.is_some()) + usize::from(outbound_id.is_some());
        let valid = match level {
            EvidenceLevel::Decision => path_count == 0 && exit_probe_id.is_none(),
            EvidenceLevel::Path => path_count == 1 && exit_probe_id.is_none(),
            EvidenceLevel::Exit => path_count == 1 && exit_probe_id.is_some(),
        };
        if !valid {
            return Err(StorageError::DecisionEvidenceInvalid);
        }
        Ok(Self {
            level,
            interface_name,
            outbound_id,
            exit_probe_id,
        })
    }

    #[must_use]
    pub const fn level(&self) -> EvidenceLevel {
        self.level
    }

    #[must_use]
    pub fn interface_name(&self) -> Option<&str> {
        self.interface_name.as_deref()
    }

    #[must_use]
    pub const fn outbound_id(&self) -> Option<&OutboundId> {
        self.outbound_id.as_ref()
    }

    #[must_use]
    pub fn exit_probe_id(&self) -> Option<&str> {
        self.exit_probe_id.as_deref()
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionDecisionInput {
    pub(crate) provider_id: String,
    pub(crate) provider_generation: u64,
    pub(crate) flow_id: String,
    pub(crate) occurred_at_unix_ms: u64,
    pub(crate) app: AppIdentity,
    pub(crate) destination: Destination,
    pub(crate) decision: Decision,
    pub(crate) evidence: DecisionEvidence,
    pub(crate) decision_latency_micros: Option<u64>,
    pub(crate) error_code: Option<String>,
}

impl ConnectionDecisionInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_id: impl Into<String>,
        provider_generation: u64,
        flow_id: impl Into<String>,
        occurred_at_unix_ms: u64,
        app: AppIdentity,
        destination: Destination,
        decision: Decision,
        evidence: DecisionEvidence,
        decision_latency_micros: Option<u64>,
        error_code: Option<String>,
    ) -> Result<Self, StorageError> {
        let provider_id = provider_id.into();
        let flow_id = flow_id.into();
        validate_input_identifier(&provider_id)?;
        validate_input_identifier(&flow_id)?;
        validate_optional_input_field(error_code.as_deref())?;
        if provider_generation == 0
            || decision.snapshot_version() == 0
            || decision_latency_micros.is_some_and(|value| value > MAX_DECISION_LATENCY_MICROS)
        {
            return Err(StorageError::ConnectionDecisionInvalid);
        }
        validate_evidence_for_decision(&evidence, &decision, error_code.as_deref())?;
        Ok(Self {
            provider_id,
            provider_generation,
            flow_id,
            occurred_at_unix_ms,
            app,
            destination,
            decision,
            evidence,
            decision_latency_micros,
            error_code,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionDecisionRecord {
    pub(crate) sequence: i64,
    pub(crate) event_id: String,
    pub(crate) persisted: PersistedDecision,
}

impl ConnectionDecisionRecord {
    #[must_use]
    pub const fn sequence(&self) -> i64 {
        self.sequence
    }

    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    #[must_use]
    pub const fn occurred_at_unix_ms(&self) -> u64 {
        self.persisted.occurred_at_unix_ms
    }

    #[must_use]
    pub fn application(&self) -> &str {
        self.persisted
            .app_display_name
            .as_deref()
            .unwrap_or(&self.persisted.app_stable_id)
    }

    #[must_use]
    pub fn app_stable_id(&self) -> &str {
        &self.persisted.app_stable_id
    }

    #[must_use]
    pub const fn app_platform(&self) -> nonproxy_model::Platform {
        self.persisted.app_platform
    }

    #[must_use]
    pub fn destination(&self) -> &str {
        &self.persisted.destination_redacted
    }

    #[must_use]
    pub const fn destination_port(&self) -> u16 {
        self.persisted.destination_port
    }

    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.persisted.transport
    }

    #[must_use]
    pub const fn action(&self) -> RouteAction {
        self.persisted.action
    }

    #[must_use]
    pub const fn evidence_level(&self) -> EvidenceLevel {
        self.persisted.evidence_level
    }

    #[must_use]
    pub fn matched_policy_id(&self) -> Option<&str> {
        self.persisted.matched_policy_id.as_deref()
    }

    #[must_use]
    pub fn matched_rule_id(&self) -> Option<&str> {
        self.persisted.matched_rule_id.as_deref()
    }

    #[must_use]
    pub const fn failure_mode(&self) -> nonproxy_model::FailureMode {
        self.persisted.failure_mode
    }

    #[must_use]
    pub fn reason_code(&self) -> &str {
        &self.persisted.reason_code
    }

    #[must_use]
    pub fn interface_name(&self) -> Option<&str> {
        self.persisted.interface_name.as_deref()
    }

    #[must_use]
    pub fn outbound_id(&self) -> Option<&str> {
        self.persisted.outbound_id.as_deref()
    }

    #[must_use]
    pub fn exit_probe_id(&self) -> Option<&str> {
        self.persisted.exit_probe_id.as_deref()
    }

    #[must_use]
    pub fn error_code(&self) -> Option<&str> {
        self.persisted.error_code.as_deref()
    }

    #[must_use]
    pub const fn decision_latency_micros(&self) -> Option<u64> {
        self.persisted.decision_latency_micros
    }

    #[must_use]
    pub fn provider_id(&self) -> &str {
        &self.persisted.provider_id
    }

    #[must_use]
    pub const fn provider_generation(&self) -> u64 {
        self.persisted.provider_generation
    }

    #[must_use]
    pub const fn snapshot_version(&self) -> u64 {
        self.persisted.snapshot_version
    }
}

fn validate_evidence_for_decision(
    evidence: &DecisionEvidence,
    decision: &Decision,
    error_code: Option<&str>,
) -> Result<(), StorageError> {
    if error_code.is_some() && evidence.level() != EvidenceLevel::Decision {
        return Err(StorageError::DecisionEvidenceInvalid);
    }
    match (decision.result().action(), evidence.level()) {
        (_, EvidenceLevel::Decision) => Ok(()),
        (RouteAction::Direct, EvidenceLevel::Path | EvidenceLevel::Exit)
            if evidence.interface_name().is_some() && evidence.outbound_id().is_none() =>
        {
            Ok(())
        }
        (RouteAction::Proxy, EvidenceLevel::Path | EvidenceLevel::Exit)
            if evidence.interface_name().is_none()
                && evidence.outbound_id() == decision.result().outbound_id() =>
        {
            Ok(())
        }
        _ => Err(StorageError::DecisionEvidenceInvalid),
    }
}

fn validate_input_identifier(value: &str) -> Result<(), StorageError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_LENGTH
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(StorageError::ConnectionDecisionInvalid);
    }
    Ok(())
}

fn validate_optional_input_field(value: Option<&str>) -> Result<(), StorageError> {
    if let Some(value) = value {
        validate_input_identifier(value)?;
        if value.len() > MAX_SHORT_FIELD_LENGTH {
            return Err(StorageError::ConnectionDecisionInvalid);
        }
    }
    Ok(())
}

fn validate_optional_evidence_field(value: Option<&str>) -> Result<(), StorageError> {
    if let Some(value) = value
        && (value.is_empty()
            || value.len() > MAX_SHORT_FIELD_LENGTH
            || value.trim() != value
            || value.chars().any(char::is_control))
    {
        return Err(StorageError::DecisionEvidenceInvalid);
    }
    Ok(())
}
