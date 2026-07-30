import Foundation
@testable import NonProxyMacPlatformSupport
import XCTest

final class MacAppIdentityResolverTests: XCTestCase {
    func testUsesSigningIdentifierAndTeamIdentifier() {
        let resolver = MacAppIdentityResolver(
            signatureInspector: FixedSignatureInspector(teamIdentifier: "TEAM123")
        )

        let identity = resolver.resolve(
            signingIdentifier: "com.example.browser",
            auditToken: Data(repeating: 7, count: 32)
        )

        XCTAssertEqual(identity.stableID, "com.example.browser")
        XCTAssertEqual(identity.signerID, "TEAM123")
    }

    func testFallsBackToExplicitUnknownIdentity() {
        let resolver = MacAppIdentityResolver(
            signatureInspector: FixedSignatureInspector(teamIdentifier: nil)
        )

        let identity = resolver.resolve(
            signingIdentifier: " ",
            auditToken: nil
        )

        XCTAssertEqual(identity.stableID, "unknown-app")
        XCTAssertNil(identity.signerID)
    }
}

private struct FixedSignatureInspector: MacCodeSignatureInspecting {
    let teamIdentifier: String?

    func teamIdentifier(for auditToken: Data) -> String? {
        teamIdentifier
    }
}
