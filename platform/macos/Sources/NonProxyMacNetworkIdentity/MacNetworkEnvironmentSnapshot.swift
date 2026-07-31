import Foundation
import Network

public struct MacNetworkEnvironmentSnapshot: @unchecked Sendable {
    public let fingerprints: [MacNetworkFingerprint]
    public let preferredInterface: NWInterface?
    public let preferredInterfaceIndex: UInt32

    private let runID: UUID
    private let generation: UInt64
    private let signature: String

    init(
        fingerprints: [MacNetworkFingerprint],
        preferredInterface: NWInterface?,
        preferredInterfaceIndex: UInt32,
        runID: UUID,
        generation: UInt64,
        signature: String
    ) {
        self.fingerprints = fingerprints
        self.preferredInterface = preferredInterface
        self.preferredInterfaceIndex = preferredInterfaceIndex
        self.runID = runID
        self.generation = generation
        self.signature = signature
    }

    public func dnsCachePartitionID(
        resolverKeys: [String] = []
    ) -> String {
        let source = [
            runID.uuidString.lowercased(),
            String(generation),
            signature,
            String(preferredInterfaceIndex),
            resolverKeys.sorted().joined(separator: ","),
        ].joined(separator: "|")
        let digest = MacNetworkFingerprintFactory.sha256(Data(source.utf8))
        return "dns-\(runID.uuidString.prefix(8).lowercased())"
            + "-\(generation)-\(digest.prefix(16))"
    }
}
