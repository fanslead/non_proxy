import Foundation
import NonProxyProviderContracts

public struct VerifiedPolicySnapshot: Sendable {
    public let wireSnapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot
    public let payload: Nonproxy_Policy_V1_CompiledPolicyPayload

    public var version: UInt64 {
        wireSnapshot.metadata.snapshotVersion
    }

    public var contentHash: Data {
        wireSnapshot.metadata.contentHash
    }

    public init(
        wireSnapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot,
        payload: Nonproxy_Policy_V1_CompiledPolicyPayload
    ) {
        self.wireSnapshot = wireSnapshot
        self.payload = payload
    }
}
