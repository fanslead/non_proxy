mod capabilities;
mod catalog;
mod change_rpc;
mod config;
mod detection;
mod error;
mod installation_rpc;
mod mapping;
mod model;
mod path_validation;
mod rpc;
mod rpc_state;
mod server;
#[cfg(unix)]
mod unix_socket;

pub use config::AdapterHostConfig;
pub use error::AdapterHostError;
pub use rpc_state::AdapterRpcService;
pub use server::{run, run_with_shutdown};
