import NetworkExtension

enum TransparentNetworkSettingsFactory {
    static func make() -> NETransparentProxyNetworkSettings {
        let settings = NETransparentProxyNetworkSettings(
            tunnelRemoteAddress: "NonProxy Local Gateway"
        )
        settings.includedNetworkRules = [
            NENetworkRule(
                remoteNetworkEndpoint: nil,
                remotePrefix: 0,
                localNetworkEndpoint: nil,
                localPrefix: 0,
                protocol: .any,
                direction: .outbound
            ),
        ]
        settings.excludedNetworkRules = []
        return settings
    }
}
