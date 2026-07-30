import Darwin
import Foundation
import NonProxyMacRuntime

@MainActor
struct NativeMessagingManifestController {
    static let manifestName = "com.nonproxy.browser"
    static let relativeDirectories = [
        "Google/Chrome/NativeMessagingHosts",
        "Chromium/NativeMessagingHosts",
        "Microsoft Edge/NativeMessagingHosts",
        "BraveSoftware/Brave-Browser/NativeMessagingHosts",
    ]

    struct Backup {
        let manifest: URL
        let contents: Data?
    }

    private let hostExecutable: URL
    private let applicationSupport: URL
    private let fileManager: FileManager

    init(
        hostExecutable: URL = Bundle.main.bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("Resources", isDirectory: true)
            .appendingPathComponent(
                MacSharedRuntimePaths.nativeMessagingHostFileName
            ),
        applicationSupport: URL = FileManager.default
            .homeDirectoryForCurrentUser
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent(
                "Application Support",
                isDirectory: true
            ),
        fileManager: FileManager = .default
    ) {
        self.hostExecutable = hostExecutable
        self.applicationSupport = applicationSupport
        self.fileManager = fileManager
    }

    func install() throws -> [Backup] {
        try validateHostExecutable()
        let contents = try manifestContents()
        var backups: [Backup] = []
        do {
            for manifest in manifestURLs() {
                let backup = try snapshot(manifest)
                backups.append(backup)
                try write(contents, to: manifest)
            }
            return backups
        } catch {
            do {
                try restore(backups)
            } catch let rollbackError {
                throw BridgeError(
                    code: "NP_MAC_NATIVE_MANIFEST_ROLLBACK_FAILED",
                    message:
                        "\(error.localizedDescription)；浏览器宿主清单回滚失败："
                        + rollbackError.localizedDescription
                )
            }
            throw error
        }
    }

    func restore(_ backups: [Backup]) throws {
        var errors: [String] = []
        for backup in backups.reversed() {
            do {
                if let contents = backup.contents {
                    try write(contents, to: backup.manifest)
                } else {
                    try removeIfPresent(backup.manifest)
                }
            } catch {
                errors.append(error.localizedDescription)
            }
        }
        if !errors.isEmpty {
            throw BridgeError(
                code: "NP_MAC_NATIVE_MANIFEST_RESTORE_FAILED",
                message: errors.joined(separator: "；")
            )
        }
    }

    func uninstall() throws {
        var errors: [String] = []
        for manifest in manifestURLs() {
            do {
                try removeIfPresent(manifest)
            } catch {
                errors.append(error.localizedDescription)
            }
        }
        if !errors.isEmpty {
            throw BridgeError(
                code: "NP_MAC_NATIVE_MANIFEST_REMOVE_FAILED",
                message: errors.joined(separator: "；")
            )
        }
    }

    private func manifestURLs() -> [URL] {
        Self.relativeDirectories.map { relative in
            applicationSupport
                .appendingPathComponent(relative, isDirectory: true)
                .appendingPathComponent(
                    "\(Self.manifestName).json"
                )
        }
    }

    private func manifestContents() throws -> Data {
        let object: [String: Any] = [
            "name": Self.manifestName,
            "description": "NonProxy 浏览器学习本地桥接",
            "path": hostExecutable.path,
            "type": "stdio",
            "allowed_origins": [
                "chrome-extension://"
                    + MacSharedRuntimePaths.chromiumExtensionID
                    + "/",
            ],
        ]
        return try JSONSerialization.data(
            withJSONObject: object,
            options: [.sortedKeys]
        )
    }

    private func snapshot(_ manifest: URL) throws -> Backup {
        do {
            let metadata = try fileManager.attributesOfItem(
                atPath: manifest.path
            )
            guard metadata[.type] as? FileAttributeType == .typeRegular,
                  !isSymbolicLink(manifest)
            else {
                throw manifestError("浏览器宿主清单类型无效。")
            }
            return Backup(
                manifest: manifest,
                contents: try Data(contentsOf: manifest)
            )
        } catch let error as CocoaError
            where error.code == .fileReadNoSuchFile
        {
            return Backup(manifest: manifest, contents: nil)
        }
    }

    private func write(_ contents: Data, to manifest: URL) throws {
        let directory = manifest.deletingLastPathComponent()
        try fileManager.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        try validateDirectoryChain(directory)
        guard !isSymbolicLink(manifest) else {
            throw manifestError("浏览器宿主清单不能是符号链接。")
        }
        try contents.write(to: manifest, options: [.atomic])
        try fileManager.setAttributes(
            [.posixPermissions: 0o644],
            ofItemAtPath: manifest.path
        )
    }

    private func removeIfPresent(_ manifest: URL) throws {
        guard fileManager.fileExists(atPath: manifest.path) else {
            return
        }
        guard !isSymbolicLink(manifest) else {
            throw manifestError("拒绝移除符号链接浏览器宿主清单。")
        }
        try fileManager.removeItem(at: manifest)
    }

    private func validateHostExecutable() throws {
        var status = stat()
        guard lstat(hostExecutable.path, &status) == 0,
              status.st_mode & S_IFMT == S_IFREG,
              status.st_mode & S_IXUSR != 0
        else {
            throw manifestError(
                "当前安装包缺少 Native Messaging Host。"
            )
        }
    }

    private func validateDirectoryChain(_ directory: URL) throws {
        let root = applicationSupport.standardizedFileURL
        var current = directory.standardizedFileURL
        guard current.path == root.path
            || current.path.hasPrefix(root.path + "/")
        else {
            throw manifestError("浏览器宿主清单目录越界。")
        }
        while true {
            var status = stat()
            guard lstat(current.path, &status) == 0,
                  status.st_mode & S_IFMT == S_IFDIR
            else {
                throw manifestError(
                    "浏览器宿主清单目录类型无效。"
                )
            }
            if current.path == root.path {
                return
            }
            current.deleteLastPathComponent()
        }
    }

    private func isSymbolicLink(_ url: URL) -> Bool {
        var status = stat()
        return lstat(url.path, &status) == 0
            && status.st_mode & S_IFMT == S_IFLNK
    }

    private func manifestError(_ message: String) -> BridgeError {
        BridgeError(
            code: "NP_MAC_NATIVE_MANIFEST_INVALID",
            message: message
        )
    }
}
