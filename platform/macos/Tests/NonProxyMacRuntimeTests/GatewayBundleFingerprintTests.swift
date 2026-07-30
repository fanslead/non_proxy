import Foundation
import Testing
@testable import NonProxyMacRuntime

struct GatewayBundleFingerprintTests {
    @Test
    func readsCanonicalFingerprintFromLaunchAgentPlist() throws {
        let fixture = try FingerprintPlistFixture(
            fingerprint: String(repeating: "a", count: 64)
        )
        defer {
            fixture.remove()
        }

        #expect(
            try GatewayBundleFingerprint.read(plistURL: fixture.url)
                == String(repeating: "a", count: 64)
        )
    }

    @Test
    func rejectsUppercaseOrMissingFingerprint() throws {
        let uppercase = try FingerprintPlistFixture(
            fingerprint: String(repeating: "A", count: 64)
        )
        defer {
            uppercase.remove()
        }

        #expect(
            throws: GatewayBundleFingerprintError.invalidFingerprint
        ) {
            try GatewayBundleFingerprint.read(plistURL: uppercase.url)
        }
    }
}

private struct FingerprintPlistFixture {
    let url: URL

    init(fingerprint: String) throws {
        url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("plist")
        let root: [String: Any] = [
            "EnvironmentVariables": [
                GatewayBundleFingerprint.environmentKey: fingerprint,
            ],
        ]
        let data = try PropertyListSerialization.data(
            fromPropertyList: root,
            format: .xml,
            options: 0
        )
        try data.write(to: url, options: .atomic)
    }

    func remove() {
        try? FileManager.default.removeItem(at: url)
    }
}
