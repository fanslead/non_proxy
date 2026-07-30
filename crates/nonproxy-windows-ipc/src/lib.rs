#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod secure_pipe;

#[cfg(windows)]
pub use secure_pipe::SecureNamedPipeFactory;
