use nonproxy_policy::OutboundCapabilities;
use nonproxy_policy_compiler::CompileCapabilities;
use nonproxy_storage::{OutboundKind, OutboundReference};

pub(crate) fn for_configured_outbounds(
    mut capabilities: CompileCapabilities,
    outbounds: &[OutboundReference],
) -> CompileCapabilities {
    for outbound in outbounds.iter().filter(|value| value.enabled()) {
        let outbound_capabilities = match outbound.kind() {
            OutboundKind::HttpConnect => OutboundCapabilities::new(true, false, true, true),
            OutboundKind::Socks5 | OutboundKind::Shadowsocks => OutboundCapabilities::full(),
            OutboundKind::Adapter => continue,
        };
        capabilities = capabilities.with_outbound(outbound.id().clone(), outbound_capabilities);
    }
    capabilities
}
