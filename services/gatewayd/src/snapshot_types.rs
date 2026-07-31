use nonproxy_model::DecisionSpec;
use nonproxy_storage::{SnapshotArtifact, SnapshotRecord};

#[derive(Clone, Debug)]
pub struct PublishedSnapshot {
    artifact: SnapshotArtifact,
    default_decision: DecisionSpec,
}

impl PublishedSnapshot {
    pub(crate) const fn new(artifact: SnapshotArtifact, default_decision: DecisionSpec) -> Self {
        Self {
            artifact,
            default_decision,
        }
    }

    #[must_use]
    pub const fn artifact(&self) -> &SnapshotArtifact {
        &self.artifact
    }

    #[must_use]
    pub const fn default_decision(&self) -> &DecisionSpec {
        &self.default_decision
    }
}

#[derive(Clone, Debug)]
pub struct ProviderSnapshot {
    record: SnapshotRecord,
    default_decision: DecisionSpec,
}

impl ProviderSnapshot {
    pub(crate) const fn new(record: SnapshotRecord, default_decision: DecisionSpec) -> Self {
        Self {
            record,
            default_decision,
        }
    }

    #[must_use]
    pub const fn record(&self) -> &SnapshotRecord {
        &self.record
    }

    #[must_use]
    pub const fn default_decision(&self) -> &DecisionSpec {
        &self.default_decision
    }
}
