import Foundation
import Testing
@testable import NonProxyMacHostBridge

@MainActor
@Suite
struct NativeMessagingManifestControllerTests {
    @Test
    func installsPinnedManifestAndRestoresPreviousContents() throws {
        let fixture = try Fixture()
        defer {
            fixture.cleanup()
        }
        let controller = fixture.controller()
        let existing = fixture.manifest(relative: "Google/Chrome")
        try FileManager.default.createDirectory(
            at: existing.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("previous".utf8).write(to: existing)

        let backups = try controller.install()

        let object = try #require(
            JSONSerialization.jsonObject(
                with: Data(contentsOf: existing)
            ) as? [String: Any]
        )
        #expect(
            object["path"] as? String == fixture.host.path
        )
        #expect(
            object["allowed_origins"] as? [String]
                == [
                    "chrome-extension://"
                        + "ldiadofihjimpkhchjicmgcfgjlgidha/",
                ]
        )
        #expect(backups.count == 4)

        try controller.restore(backups)
        #expect(
            try String(contentsOf: existing, encoding: .utf8)
                == "previous"
        )
        #expect(
            !FileManager.default.fileExists(
                atPath: fixture.manifest(relative: "Chromium").path
            )
        )
    }

    @Test
    func rejectsMissingOrNonExecutableHost() throws {
        let fixture = try Fixture(makeExecutable: false)
        defer {
            fixture.cleanup()
        }

        #expect(throws: BridgeError.self) {
            try fixture.controller().install()
        }
    }
}

@MainActor
private struct Fixture {
    let root: URL
    let applicationSupport: URL
    let host: URL

    init(makeExecutable: Bool = true) throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent(
                "nonproxy-manifest-\(UUID().uuidString)",
                isDirectory: true
            )
        applicationSupport = root.appendingPathComponent(
            "Application Support",
            isDirectory: true
        )
        host = root.appendingPathComponent("native-host")
        try FileManager.default.createDirectory(
            at: root,
            withIntermediateDirectories: false
        )
        try Data("#!/bin/sh\n".utf8).write(to: host)
        try FileManager.default.setAttributes(
            [.posixPermissions: makeExecutable ? 0o700 : 0o600],
            ofItemAtPath: host.path
        )
    }

    func controller() -> NativeMessagingManifestController {
        NativeMessagingManifestController(
            hostExecutable: host,
            applicationSupport: applicationSupport
        )
    }

    func manifest(relative: String) -> URL {
        applicationSupport
            .appendingPathComponent(
                "\(relative)/NativeMessagingHosts",
                isDirectory: true
            )
            .appendingPathComponent(
                "\(NativeMessagingManifestController.manifestName).json"
            )
    }

    func cleanup() {
        try? FileManager.default.removeItem(at: root)
    }
}
