#[cfg(windows)]
mod incoming;
#[cfg(windows)]
mod secure_pipe;
mod validation;

#[cfg(windows)]
pub use incoming::{ConnectedNamedPipe, NamedPipeIncoming};
#[cfg(windows)]
pub use secure_pipe::SecureNamedPipeFactory;
pub use validation::{validate_nonproxy_pipe_name, validate_pipe_sddl};
