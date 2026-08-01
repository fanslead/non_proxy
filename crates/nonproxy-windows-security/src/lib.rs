mod validation;

#[cfg(windows)]
mod acl;
#[cfg(windows)]
mod atomic_file;
#[cfg(windows)]
mod path;
#[cfg(windows)]
mod user;

#[cfg(windows)]
pub use acl::{protect_current_user_directory, protect_current_user_file};
#[cfg(windows)]
pub use atomic_file::replace_file_atomically;
#[cfg(windows)]
pub use path::{validate_regular_directory, validate_regular_file};
#[cfg(windows)]
pub use user::current_process_user_sid;
pub use validation::validate_interactive_user_sid;
