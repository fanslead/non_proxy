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
    capabilities_for_platform(current_platform(), client, version)
}

pub(crate) fn client_supported_on_current_platform(client: AdapterClient) -> bool {
    client_supported_on_platform(current_platform(), client)
}

pub(crate) fn hot_reload_supported(client: AdapterClient, version: AdapterVersion) -> bool {
    capabilities(client, version).contains(&AdapterCapability::HotReload)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdapterHostPlatform {
    MacOs,
    Windows,
    OtherUnix,
    Unsupported,
}

fn current_platform() -> AdapterHostPlatform {
    if cfg!(target_os = "macos") {
        AdapterHostPlatform::MacOs
    } else if cfg!(windows) {
        AdapterHostPlatform::Windows
    } else if cfg!(unix) {
        AdapterHostPlatform::OtherUnix
    } else {
        AdapterHostPlatform::Unsupported
    }
}

fn capabilities_for_platform(
    platform: AdapterHostPlatform,
    client: AdapterClient,
    version: AdapterVersion,
) -> Vec<AdapterCapability> {
    if !client_supported_on_platform(platform, client) {
        return Vec::new();
    }
    let values = match client {
        AdapterClient::Surge => SurgeRenderer.capabilities(version),
        AdapterClient::Mihomo => MihomoRenderer.capabilities(version),
        AdapterClient::SingBox => SingBoxRenderer.capabilities(version),
    };
    values
        .into_iter()
        .map(map_capability)
        .filter(|capability| {
            !(platform == AdapterHostPlatform::Windows
                && client == AdapterClient::SingBox
                && *capability == AdapterCapability::HotReload)
        })
        .collect()
}

const fn client_supported_on_platform(
    platform: AdapterHostPlatform,
    client: AdapterClient,
) -> bool {
    match (platform, client) {
        (AdapterHostPlatform::MacOs, _) => true,
        (AdapterHostPlatform::Windows | AdapterHostPlatform::OtherUnix, AdapterClient::Surge) => {
            false
        }
        (AdapterHostPlatform::Windows | AdapterHostPlatform::OtherUnix, _) => true,
        (AdapterHostPlatform::Unsupported, _) => false,
    }
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

#[cfg(test)]
mod tests {
    use nonproxy_adapter_api::{AdapterClient, AdapterVersion};
    use nonproxy_proto::adapter::v1::AdapterCapability;

    use super::{AdapterHostPlatform, capabilities_for_platform};

    const VERSION: AdapterVersion = AdapterVersion::new(1, 20, 0);

    #[test]
    fn surge_is_only_advertised_on_macos() {
        let surge_version = AdapterVersion::new(6, 1, 0);
        assert!(
            !capabilities_for_platform(
                AdapterHostPlatform::MacOs,
                AdapterClient::Surge,
                surge_version,
            )
            .is_empty()
        );
        assert!(
            capabilities_for_platform(
                AdapterHostPlatform::Windows,
                AdapterClient::Surge,
                surge_version,
            )
            .is_empty()
        );
    }

    #[test]
    fn windows_sing_box_does_not_claim_unsupported_hot_reload() {
        let capabilities = capabilities_for_platform(
            AdapterHostPlatform::Windows,
            AdapterClient::SingBox,
            VERSION,
        );

        assert!(!capabilities.is_empty());
        assert!(!capabilities.contains(&AdapterCapability::HotReload));
    }

    #[test]
    fn windows_mihomo_keeps_http_reload_capability() {
        let capabilities =
            capabilities_for_platform(AdapterHostPlatform::Windows, AdapterClient::Mihomo, VERSION);

        assert!(capabilities.contains(&AdapterCapability::HotReload));
    }

    #[test]
    fn unsupported_platform_advertises_no_clients() {
        assert!(
            capabilities_for_platform(
                AdapterHostPlatform::Unsupported,
                AdapterClient::Mihomo,
                VERSION,
            )
            .is_empty()
        );
    }
}
