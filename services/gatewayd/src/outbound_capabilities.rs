use nonproxy_model::OutboundGroupSpec;
use nonproxy_policy::OutboundCapabilities;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{OutboundGroup, OutboundKind, OutboundReference};

use crate::GatewayError;

pub(crate) fn for_configured_outbounds(
    mut capabilities: CompileCapabilities,
    outbounds: &[OutboundReference],
    groups: &[OutboundGroup],
) -> Result<CompileCapabilities, GatewayError> {
    for outbound in outbounds.iter().filter(|value| value.enabled()) {
        let outbound_capabilities = match outbound.kind() {
            OutboundKind::HttpConnect => OutboundCapabilities::new(true, false, true, true),
            OutboundKind::Socks5 | OutboundKind::Shadowsocks => OutboundCapabilities::full(),
            OutboundKind::Adapter => continue,
        };
        capabilities = capabilities.with_outbound(outbound.id().clone(), outbound_capabilities);
    }
    for group in groups {
        let spec = OutboundGroupSpec::new(
            group.id().clone(),
            group.revision(),
            group.members().to_vec(),
        )?;
        capabilities = capabilities.with_outbound_group(spec)?;
    }
    Ok(capabilities)
}
