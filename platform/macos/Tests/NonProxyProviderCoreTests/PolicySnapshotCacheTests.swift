import Foundation
@testable import NonProxyProviderCore
import XCTest

final class PolicySnapshotCacheTests: XCTestCase {
    func testRoundTripsVerifiedSnapshotWithRestrictedPermissions() async throws {
        let directory = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let cache = try PolicySnapshotCache(
            directory: directory,
            providerName: "transparent-proxy"
        )
        let verified = try SnapshotValidator.validate(SnapshotFixtures.snapshot())

        try await cache.save(verified)
        let loaded = try await cache.load()

        XCTAssertEqual(loaded?.version, verified.version)
        let file = directory.appendingPathComponent("transparent-proxy-snapshot.pb")
        let attributes = try FileManager.default.attributesOfItem(atPath: file.path)
        XCTAssertEqual(
            (attributes[.posixPermissions] as? NSNumber)?.intValue,
            0o600
        )
    }

    func testRejectsSymbolicLinkCacheDirectory() async throws {
        let root = temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let realDirectory = root.appendingPathComponent("real", isDirectory: true)
        let linkedDirectory = root.appendingPathComponent("linked", isDirectory: true)
        try FileManager.default.createDirectory(
            at: realDirectory,
            withIntermediateDirectories: true
        )
        try FileManager.default.createSymbolicLink(
            at: linkedDirectory,
            withDestinationURL: realDirectory
        )
        let cache = try PolicySnapshotCache(
            directory: linkedDirectory,
            providerName: "dns-proxy"
        )
        let verified = try SnapshotValidator.validate(SnapshotFixtures.snapshot())

        do {
            try await cache.save(verified)
            XCTFail("符号链接缓存目录不应被接受")
        } catch let error as ProviderError {
            XCTAssertEqual(error.code, "NP_PROVIDER_SNAPSHOT_CACHE_FAILED")
        }
    }

    private func temporaryDirectory() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent(
            "nonproxy-cache-tests-\(UUID().uuidString)",
            isDirectory: true
        )
    }
}
