mod clock;
mod config;
mod control_mapping;
mod control_rpc_helpers;
mod control_rpc_service;
mod control_service;
mod credential_store;
mod database_executor;
#[cfg(any(test, windows))]
mod decision_event;
mod decision_ingest;
mod decision_rpc;
mod decision_snapshot_cache;
mod decision_telemetry;
mod diagnostics_document;
mod diagnostics_export;
#[cfg(test)]
mod diagnostics_export_tests;
mod diagnostics_file;
mod diagnostics_labels;
mod diagnostics_redaction;
#[cfg(any(test, windows))]
mod dns_policy;
mod dns_service;
mod error;
mod event_hub;
mod exit_probe;
mod exit_probe_direct;
mod exit_probe_gateway;
mod exit_probe_rpc;
#[cfg(any(unix, windows))]
mod flow_server;
mod gateway;
mod learning_confirmation_gateway;
mod learning_contract;
mod learning_gateway;
mod learning_rpc;
#[cfg(any(test, windows))]
mod local_dns_server;
mod network_profile_gateway;
mod network_profile_rpc;
mod outbound_capabilities;
mod outbound_catalog_gateway;
mod outbound_health;
mod outbound_import;
mod outbound_import_service;
#[cfg(test)]
mod outbound_import_tests;
mod outbound_import_uri;
mod outbound_probe;
mod outbound_probe_tls;
mod policy_catalog_gateway;
mod proto_policy;
mod provider_decision_rpc;
mod provider_health;
mod provider_requirements;
mod provider_service;
mod provider_session;
mod routing_gateway;
mod routing_rpc;
mod runtime_events;
mod runtime_identity;
mod runtime_override_gateway;
mod runtime_override_rpc;
mod runtime_policy;
mod server;
mod session_capability;
mod snapshot_builder;
mod snapshot_payload;
mod snapshot_types;
mod system_policies;
mod system_rpc;
mod system_snapshot_gateway;
#[cfg(unix)]
mod unix_socket;
#[cfg(windows)]
mod windows_capture;
mod windows_config;
#[cfg(windows)]
mod windows_service;

pub use config::GatewayConfig;
pub use error::GatewayError;
pub use event_hub::EventHub;
pub use gateway::{Gateway, GatewayStatus};
pub use learning_confirmation_gateway::LearningConfirmationResult;
pub use routing_gateway::StagedRoutingSettings;
pub use runtime_policy::{RuntimePolicyCatalog, RuntimePolicyRecord, RuntimePolicyState};
pub use server::run;
pub use snapshot_payload::{SNAPSHOT_PAYLOAD_FORMAT, decode as decode_snapshot_payload};
pub use snapshot_types::{ProviderSnapshot, PublishedSnapshot};
pub use windows_config::WindowsTransportConfig;
#[cfg(windows)]
pub use windows_service::run_dispatcher as run_windows_service_dispatcher;
