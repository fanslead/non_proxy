mod app_identity;
mod connection;
mod decision;
mod destination;
mod error;
mod ids;
mod policy;

pub use app_identity::{AppIdentity, AppMatcher, Platform};
pub use connection::ConnectionContext;
pub use decision::{Decision, DecisionSpec, FailureMode, RouteAction};
pub use destination::{
    Cidr, Destination, DomainMatchKind, DomainMatcher, DomainName, IpFamily, PortRange, Transport,
};
pub use error::ModelError;
pub use ids::{NetworkProfileId, OutboundId, PolicyId, RuleId};
pub use policy::{
    NetworkMatcher, Policy, PolicyMatch, PolicyMetadata, PolicyOrigin, PolicySourceKind,
    PolicyValidation,
};
