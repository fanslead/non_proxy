import Foundation
import NonProxyNativeMessaging
import XCTest

final class NativeHostRuntimeConfigurationTests: XCTestCase {
    func testReadsOnlyOwnerPrivateRegularCapability() throws {
        let directory = try temporaryDirectory()
        defer {
            try? FileManager.default.removeItem(at: directory)
        }
        let capability = directory.appendingPathComponent(
            "session.capability"
        )
        try Data(repeating: 7, count: 32).write(to: capability)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: capability.path
        )
        let configuration = try NativeHostRuntimeConfiguration(
            controlSocket: directory.appendingPathComponent(
                "gatewayd.sock"
            ),
            controlCapability: capability
        )

        XCTAssertEqual(
            try configuration.readControlCapability(),
            Data(repeating: 7, count: 32)
        )

        try FileManager.default.setAttributes(
            [.posixPermissions: 0o644],
            ofItemAtPath: capability.path
        )
        XCTAssertThrowsError(
            try configuration.readControlCapability()
        )
    }

    func testRejectsSymbolicLinkCapability() throws {
        let directory = try temporaryDirectory()
        defer {
            try? FileManager.default.removeItem(at: directory)
        }
        let target = directory.appendingPathComponent("target")
        let link = directory.appendingPathComponent(
            "session.capability"
        )
        try Data(repeating: 7, count: 32).write(to: target)
        try FileManager.default.createSymbolicLink(
            at: link,
            withDestinationURL: target
        )
        let configuration = try NativeHostRuntimeConfiguration(
            controlSocket: directory.appendingPathComponent(
                "gatewayd.sock"
            ),
            controlCapability: link
        )

        XCTAssertThrowsError(
            try configuration.readControlCapability()
        )
    }

    func testRejectsRegularFileInPlaceOfControlSocket() throws {
        let directory = try temporaryDirectory()
        defer {
            try? FileManager.default.removeItem(at: directory)
        }
        let socket = directory.appendingPathComponent("gatewayd.sock")
        try Data().write(to: socket)
        let configuration = try NativeHostRuntimeConfiguration(
            controlSocket: socket,
            controlCapability: directory.appendingPathComponent(
                "session.capability"
            )
        )

        XCTAssertThrowsError(
            try configuration.validateControlSocket()
        )
    }

    private func temporaryDirectory() throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(
                "nonproxy-native-\(UUID().uuidString)",
                isDirectory: true
            )
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false
        )
        return directory
    }
}
