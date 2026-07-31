mod atomic_file;
mod digest;
mod error;
mod host;
mod identifier;
mod integrated;
mod integrated_state;
mod manifest;
mod path_guard;
mod recovery;
mod renderer_catalog;
mod transaction_checks;
mod types;

pub use error::AdapterTransactionError;
pub use host::AdapterTransactionManager;
pub use types::{
    AdapterInstallation, ApplyOutcome, ChangeInstallation, IntegratedCandidate,
    IntegratedPreparation, PreparedChange, RollbackOutcome, VerificationOutcome,
};
