#[cfg(windows)]
mod incoming;
#[cfg(windows)]
mod secure_pipe;
#[cfg(windows)]
mod user;
mod validation;

#[cfg(windows)]
pub use incoming::{ConnectedNamedPipe, NamedPipeIncoming};
#[cfg(windows)]
pub use secure_pipe::SecureNamedPipeFactory;
#[cfg(windows)]
pub use user::current_process_user_sid;
pub use validation::{
    adapter_pipe_name_for_user_sid, adapter_pipe_sddl_for_user_sid, validate_nonproxy_pipe_name,
    validate_nonproxy_user_sid, validate_pipe_sddl,
};
