use nonproxy_adapter_api::{
    AdapterCapability as DomainCapability, AdapterClient, AdapterRenderer, AdapterVersion,
};
use nonproxy_adapter_mihomo::MihomoRenderer;
use nonproxy_adapter_sing_box::SingBoxRenderer;
use nonproxy_adapter_surge::SurgeRenderer;
use nonproxy_proto::adapter::v1::AdapterCapability;

pub(crate) fn capabilities(
    client: AdapterClient,
    version: AdapterVersion,
) -> Vec<AdapterCapability> {
    let values = match client {
        AdapterClient::Surge => SurgeRenderer.capabilities(version),
        AdapterClient::Mihomo => MihomoRenderer.capabilities(version),
        AdapterClient::SingBox => SingBoxRenderer.capabilities(version),
    };
    values.into_iter().map(map_capability).collect()
}

fn map_capability(value: DomainCapability) -> AdapterCapability {
    match value {
        DomainCapability::ApplicationRule => AdapterCapability::AppRule,
        DomainCapability::DomainRule => AdapterCapability::DomainRule,
        DomainCapability::CidrRule => AdapterCapability::CidrRule,
        DomainCapability::HotReload => AdapterCapability::HotReload,
        DomainCapability::PathEvidence => AdapterCapability::PathEvidence,
    }
}
