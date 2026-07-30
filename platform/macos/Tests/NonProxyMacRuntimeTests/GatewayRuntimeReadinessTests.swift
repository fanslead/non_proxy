import Foundation
import Testing
@testable import NonProxyMacRuntime

struct GatewayRuntimeReadinessTests {
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
            try GatewayRuntimeReadiness.inspect(paths: paths)
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
            try GatewayRuntimeReadiness.inspect(paths: paths)
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
}
