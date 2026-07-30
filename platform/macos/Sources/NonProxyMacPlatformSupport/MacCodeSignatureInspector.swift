import Foundation
import Security

public protocol MacCodeSignatureInspecting: Sendable {
    func teamIdentifier(for auditToken: Data) -> String?
}

public struct MacCodeSignatureInspector: MacCodeSignatureInspecting {
    public init() {}

    public func teamIdentifier(for auditToken: Data) -> String? {
        guard !auditToken.isEmpty else {
            return nil
        }
        let attributes = [
            kSecGuestAttributeAudit: auditToken as CFData,
        ] as CFDictionary
        var code: SecCode?
        guard SecCodeCopyGuestWithAttributes(
            nil,
            attributes,
            [],
            &code
        ) == errSecSuccess, let code else {
            return nil
        }

        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess,
              let staticCode
        else {
            return nil
        }
        var information: CFDictionary?
        guard SecCodeCopySigningInformation(
            staticCode,
            SecCSFlags(rawValue: kSecCSSigningInformation),
            &information
        ) == errSecSuccess,
            let values = information as? [CFString: Any],
            let teamIdentifier = values[kSecCodeInfoTeamIdentifier] as? String,
            !teamIdentifier.isEmpty
        else {
            return nil
        }
        return teamIdentifier
    }
}
