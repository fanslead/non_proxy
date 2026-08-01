#[cfg(windows)]
mod incoming;
#[cfg(windows)]
mod secure_pipe;
mod validation;

#[cfg(windows)]
pub use incoming::{ConnectedNamedPipe, NamedPipeIncoming};
#[cfg(windows)]
pub use nonproxy_windows_security::current_process_user_sid;
pub use nonproxy_windows_security::validate_interactive_user_sid as validate_nonproxy_user_sid;
#[cfg(windows)]
pub use secure_pipe::SecureNamedPipeFactory;
pub use validation::{
    adapter_pipe_name_for_user_sid, adapter_pipe_sddl_for_user_sid, validate_nonproxy_pipe_name,
    validate_pipe_sddl,
};
