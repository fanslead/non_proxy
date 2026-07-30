import Foundation
@testable import NonProxyMacPlatformSupport
import NonProxyProviderCore
import XCTest

final class MacProviderPathsTests: XCTestCase {
    func testReadsOwnerOnlyRegularCapability() throws {
        let fixture = try CapabilityFixture()
        defer { fixture.cleanup() }
        try fixture.writeCapability(permissions: 0o600)

        let capability = try fixture.paths.readBootstrapCapability()

        XCTAssertEqual(capability, Data(repeating: 9, count: 32))
        XCTAssertEqual(
            fixture.paths.flowSocketPath,
            fixture.root.appendingPathComponent("gatewayd-flow.sock").path
        )
    }

    func testRejectsGroupReadableCapability() throws {
        let fixture = try CapabilityFixture()
        defer { fixture.cleanup() }
        try fixture.writeCapability(permissions: 0o640)

        XCTAssertThrowsError(try fixture.paths.readBootstrapCapability()) {
            XCTAssertEqual(
                ($0 as? ProviderError)?.code,
                "NP_PROVIDER_CONFIGURATION_INVALID"
            )
        }
    }

    func testRejectsCapabilitySymbolicLink() throws {
        let fixture = try CapabilityFixture()
        defer { fixture.cleanup() }
        let target = fixture.root.appendingPathComponent("target")
        try Data(repeating: 9, count: 32).write(to: target)
        try FileManager.default.createSymbolicLink(
            at: fixture.capability,
            withDestinationURL: target
        )

        XCTAssertThrowsError(try fixture.paths.readBootstrapCapability())
    }

    func testRejectsStateDirectorySymbolicLink() throws {
        let fixture = try CapabilityFixture()
        defer { fixture.cleanup() }
        try fixture.writeCapability(permissions: 0o600)
        let linked = fixture.root
            .deletingLastPathComponent()
            .appendingPathComponent("linked-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: linked) }
        try FileManager.default.createSymbolicLink(
            at: linked,
            withDestinationURL: fixture.root
        )
        let paths = try MacProviderPaths(stateDirectory: linked)

        XCTAssertThrowsError(try paths.readBootstrapCapability())
    }
}

private struct CapabilityFixture {
    let root: URL
    let capability: URL
    let paths: MacProviderPaths

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "nonproxy-provider-paths-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        capability = root.appendingPathComponent("provider.capability")
        paths = try MacProviderPaths(stateDirectory: root)
    }

    func writeCapability(permissions: Int) throws {
        try Data(repeating: 9, count: 32).write(to: capability)
        try FileManager.default.setAttributes(
            [.posixPermissions: permissions],
            ofItemAtPath: capability.path
        )
    }

    func cleanup() {
        try? FileManager.default.removeItem(at: root)
    }
}
