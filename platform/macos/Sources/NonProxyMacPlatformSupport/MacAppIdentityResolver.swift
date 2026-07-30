import Foundation
import NetworkExtension
import NonProxyProviderCore

public struct MacAppIdentityResolver: Sendable {
    private let signatureInspector: any MacCodeSignatureInspecting

    public init(
        signatureInspector: any MacCodeSignatureInspecting = MacCodeSignatureInspector()
    ) {
        self.signatureInspector = signatureInspector
    }

    public func resolve(metadata: NEFlowMetaData) -> PolicyAppIdentity {
        resolve(
            signingIdentifier: metadata.sourceAppSigningIdentifier,
            auditToken: metadata.sourceAppAuditToken
        )
    }

    public func resolve(
        signingIdentifier: String,
        auditToken: Data?
    ) -> PolicyAppIdentity {
        let stableID = Self.normalizedSigningIdentifier(signingIdentifier)
            ?? PolicyAppIdentity.unknown.stableID
        let signerID = auditToken.flatMap {
            signatureInspector.teamIdentifier(for: $0)
        }
        return PolicyAppIdentity(
            stableID: stableID,
            signerID: signerID
        )
    }

    private static func normalizedSigningIdentifier(
        _ value: String
    ) -> String? {
        guard !value.isEmpty,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines),
              value.utf8.count <= 255,
              value.unicodeScalars.allSatisfy({
                  !CharacterSet.controlCharacters.contains($0)
              })
        else {
            return nil
        }
        return value
    }
}
