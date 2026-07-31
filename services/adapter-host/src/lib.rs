mod candidate_validation;
mod capabilities;
mod catalog;
mod change_rpc;
mod client_paths;
mod config;
mod detection;
mod error;
mod installation_rpc;
mod mapping;
mod model;
mod path_validation;
mod process_runner;
mod reload;
mod reload_rpc;
mod rpc;
mod rpc_state;
mod runtime_identity;
mod server;
#[cfg(unix)]
mod unix_socket;

pub use config::AdapterHostConfig;
pub use error::AdapterHostError;
pub use rpc_state::AdapterRpcService;
pub use server::{run, run_with_shutdown};
