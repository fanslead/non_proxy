import Foundation
import Testing
@testable import NonProxyMacRuntime

struct GatewayRuntimeReadinessTests {
    private let fingerprint = String(repeating: "a", count: 64)

    @Test
    func rejectsRuntimeDirectoryWithLoosePermissions() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: directory.path
        )
        defer {
            try? FileManager.default.removeItem(at: directory)
        }
        let paths = try MacSharedRuntimePaths(
            stateDirectory: directory
        )

        #expect(
            throws: GatewayRuntimeReadinessError.invalidStateDirectory
        ) {
            try GatewayRuntimeReadiness.inspect(
                paths: paths,
                expectedFingerprint: fingerprint
            )
        }
    }

    @Test
    func rejectsRuntimeWithoutSockets() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: directory.path
        )
        defer {
            try? FileManager.default.removeItem(at: directory)
        }
        let paths = try MacSharedRuntimePaths(
            stateDirectory: directory
        )

        #expect(throws: GatewayRuntimeReadinessError.invalidSocket) {
            try GatewayRuntimeReadiness.inspect(
                paths: paths,
                expectedFingerprint: fingerprint
            )
        }
    }

    @Test
    func rejectsCapabilityWithLoosePermissions() throws {
        let fixture = try RuntimeFixture()
        defer {
            fixture.remove()
        }
        try Data(repeating: 7, count: 32).write(
            to: fixture.paths.controlCapability
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o644],
            ofItemAtPath: fixture.paths.controlCapability.path
        )

        #expect(throws: GatewayRuntimeReadinessError.invalidCapability) {
            try GatewayRuntimeReadiness.inspectCapability(
                fixture.paths.controlCapability,
                fileManager: .default
            )
        }
    }

    @Test
    func acceptsMatchingLiveRuntimeIdentity() throws {
        let fixture = try RuntimeFixture()
        defer {
            fixture.remove()
        }
        try fixture.writeIdentity(
            fingerprint: fingerprint,
            processID: UInt32(getpid())
        )

        try GatewayRuntimeReadiness.inspectRuntimeIdentity(
            fixture.paths.runtimeIdentity,
            expectedFingerprint: fingerprint,
            fileManager: .default
        )
    }

    @Test
    func reportsRuntimeFingerprintMismatch() throws {
        let fixture = try RuntimeFixture()
        defer {
            fixture.remove()
        }
        try fixture.writeIdentity(
            fingerprint: String(repeating: "b", count: 64),
            processID: UInt32(getpid())
        )

        #expect(
            throws: GatewayRuntimeReadinessError.fingerprintMismatch
        ) {
            try GatewayRuntimeReadiness.inspectRuntimeIdentity(
                fixture.paths.runtimeIdentity,
                expectedFingerprint: fingerprint,
                fileManager: .default
            )
        }
    }

    @Test
    func rejectsIdentityForStoppedProcess() throws {
        let fixture = try RuntimeFixture()
        defer {
            fixture.remove()
        }
        try fixture.writeIdentity(
            fingerprint: fingerprint,
            processID: 42
        )

        #expect(
            throws: GatewayRuntimeReadinessError.invalidRuntimeIdentity
        ) {
            try GatewayRuntimeReadiness.inspectRuntimeIdentity(
                fixture.paths.runtimeIdentity,
                expectedFingerprint: fingerprint,
                fileManager: .default,
                processIsAlive: { _ in false }
            )
        }
    }
}

private struct RuntimeFixture {
    let paths: MacSharedRuntimePaths

    init() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: directory.path
        )
        paths = try MacSharedRuntimePaths(stateDirectory: directory)
    }

    func remove() {
        try? FileManager.default.removeItem(at: paths.stateDirectory)
    }

    func writeIdentity(
        fingerprint: String,
        processID: UInt32
    ) throws {
        let payload: [String: Any] = [
            "schemaVersion": 1,
            "bundleFingerprint": fingerprint,
            "processId": processID,
            "semanticVersion": "1.0.0",
            "buildId": "test",
        ]
        let data = try JSONSerialization.data(withJSONObject: payload)
        try data.write(to: paths.runtimeIdentity, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: paths.runtimeIdentity.path
        )
    }
}
