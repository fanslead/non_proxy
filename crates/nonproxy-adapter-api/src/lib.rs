mod error;
mod model;
mod renderer;

pub use error::AdapterContractError;
pub use model::{
    AdapterCapability, AdapterClient, AdapterVersion, DomainSelectorKind, NormalizedPolicy,
    NormalizedRule, PolicyAction, RuleSelector,
};
pub use renderer::{AdapterRenderer, RenderedRules};
