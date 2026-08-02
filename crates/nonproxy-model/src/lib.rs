mod app_identity;
mod connection;
mod decision;
mod destination;
mod error;
mod ids;
mod network_profile;
mod outbound_group;
mod policy;
mod runtime_override;

pub use app_identity::{AppIdentity, AppMatcher, Platform};
pub use connection::ConnectionContext;
pub use decision::{Decision, DecisionSpec, FailureMode, ProxyTarget, RouteAction};
pub use destination::{
    Cidr, Destination, DomainMatchKind, DomainMatcher, DomainName, IpFamily, PortRange, Transport,
};
pub use error::ModelError;
pub use ids::{NetworkProfileId, OutboundGroupId, OutboundId, PolicyId, RuleId};
pub use network_profile::{
    NetworkFingerprint, NetworkFingerprintKind, NetworkProfileBinding, NetworkProfileReference,
};
pub use outbound_group::OutboundGroupSpec;
pub use policy::{
    NetworkMatcher, Policy, PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind,
    PolicyValidation,
};
pub use runtime_override::{RuntimeOverrideMode, RuntimeRoutingOverride};
