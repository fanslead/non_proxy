import Foundation
import NonProxyProviderContracts

public struct PolicyNetworkFingerprint: Hashable, Sendable {
    public let kind: Nonproxy_Policy_V1_NetworkFingerprintKind
    public let value: String

    public init(
        kind: Nonproxy_Policy_V1_NetworkFingerprintKind,
        value: String
    ) throws {
        switch kind {
        case .wifiSsidSha256, .defaultGatewaySha256:
            guard value.utf8.count == 64,
                  value.utf8.allSatisfy({
                      (48 ... 57).contains($0) || (97 ... 102).contains($0)
                  })
            else {
                throw ProviderError.invalidConfiguration(
                    "运行时网络哈希指纹无效"
                )
            }
        case .interfaceClass:
            guard ["wifi", "ethernet", "cellular", "other"]
                .contains(value)
            else {
                throw ProviderError.invalidConfiguration(
                    "运行时网络接口类型无效"
                )
            }
        default:
            throw ProviderError.invalidConfiguration(
                "运行时网络指纹类型无效"
            )
        }
        self.kind = kind
        self.value = value
    }
}

enum PolicyNetworkProfileResolver {
    static func resolve(
        in snapshot: VerifiedPolicySnapshot,
        fingerprints: [PolicyNetworkFingerprint]
    ) -> String? {
        var catalog: [FingerprintKey: String] = [:]
        for profile in snapshot.payload.networkProfiles {
            let key = FingerprintKey(
                kind: profile.fingerprintKind,
                value: profile.fingerprintValue
            )
            catalog[key] = profile.id
        }
        return Set(fingerprints)
            .sorted(by: preferred)
            .lazy
            .compactMap {
                catalog[FingerprintKey(kind: $0.kind, value: $0.value)]
            }
            .first
    }

    private static func preferred(
        _ left: PolicyNetworkFingerprint,
        _ right: PolicyNetworkFingerprint
    ) -> Bool {
        let leftRank = rank(left.kind)
        let rightRank = rank(right.kind)
        if leftRank != rightRank {
            return leftRank < rightRank
        }
        return left.value < right.value
    }

    private static func rank(
        _ kind: Nonproxy_Policy_V1_NetworkFingerprintKind
    ) -> Int {
        switch kind {
        case .wifiSsidSha256:
            0
        case .defaultGatewaySha256:
            1
        case .interfaceClass:
            2
        default:
            3
        }
    }
}

private struct FingerprintKey: Hashable {
    let kind: Nonproxy_Policy_V1_NetworkFingerprintKind
    let value: String
}
