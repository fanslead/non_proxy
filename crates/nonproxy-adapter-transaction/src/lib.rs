mod atomic_file;
mod digest;
mod error;
mod host;
mod identifier;
mod integrated;
mod manifest;
mod path_guard;
mod recovery;
mod renderer_catalog;
mod types;

pub use error::AdapterTransactionError;
pub use host::AdapterTransactionManager;
pub use types::{
    AdapterInstallation, ApplyOutcome, ChangeInstallation, IntegratedCandidate, PreparedChange,
    RollbackOutcome, VerificationOutcome,
};
