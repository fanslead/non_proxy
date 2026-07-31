import NonProxyMacNetworkIdentity
import NonProxyProviderContracts
import NonProxyProviderCore

public extension MacNetworkEnvironmentSnapshot {
    var policyFingerprints: [PolicyNetworkFingerprint] {
        fingerprints.compactMap { fingerprint in
            let kind: Nonproxy_Policy_V1_NetworkFingerprintKind
            switch fingerprint.kind {
            case .wifiSSIDHash:
                kind = .wifiSsidSha256
            case .defaultGatewayHash:
                kind = .defaultGatewaySha256
            case .interfaceClass:
                kind = .interfaceClass
            }
            return try? PolicyNetworkFingerprint(
                kind: kind,
                value: fingerprint.value
            )
        }
    }
}
