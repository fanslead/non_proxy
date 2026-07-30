import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import XCTest

final class ProviderConfigurationTests: XCTestCase {
    func testRequiresMatchingMacProviderKindAndComponent() {
        XCTAssertThrowsError(
            try ProviderConfiguration(
                kind: .transparentProxy,
                component: .dnsProxy,
                socketPath: "/tmp/nonproxy.sock",
                bootstrapCapability: Data(repeating: 1, count: 32),
                cacheDirectory: FileManager.default.temporaryDirectory,
                semanticVersion: "1.0.0",
                buildID: "test"
            )
        )
    }

    func testAcceptsMatchingProviderConfiguration() {
        XCTAssertNoThrow(
            try ProviderConfiguration(
                kind: .dnsProxy,
                component: .dnsProxy,
                socketPath: "/tmp/nonproxy.sock",
                bootstrapCapability: Data(repeating: 1, count: 32),
                cacheDirectory: FileManager.default.temporaryDirectory,
                semanticVersion: "1.0.0",
                buildID: "test"
            )
        )
    }
}
