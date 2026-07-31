use nonproxy_adapter_api::{
    AdapterClient, AdapterContractError, AdapterRenderer, AdapterVersion, NormalizedPolicy,
    RenderedRules,
};
use nonproxy_adapter_mihomo::MihomoRenderer;
use nonproxy_adapter_sing_box::SingBoxRenderer;
use nonproxy_adapter_surge::SurgeRenderer;

pub(crate) fn render(
    client: AdapterClient,
    version: AdapterVersion,
    policy: &NormalizedPolicy,
) -> Result<RenderedRules, AdapterContractError> {
    match client {
        AdapterClient::Surge => SurgeRenderer.render(version, policy),
        AdapterClient::Mihomo => MihomoRenderer.render(version, policy),
        AdapterClient::SingBox => SingBoxRenderer.render(version, policy),
    }
}
