import Darwin
import Foundation

public struct NativeHostRuntimeConfiguration: Equatable, Sendable {
    public let controlSocket: URL
    public let controlCapability: URL

    public init(
        controlSocket: URL,
        controlCapability: URL
    ) throws {
        guard controlSocket.isFileURL,
              controlCapability.isFileURL,
              controlSocket.path.hasPrefix("/"),
              controlCapability.path.hasPrefix("/")
        else {
            throw NativeMessagingError.runtimeUnavailable(
                "本地控制路径无效。"
            )
        }
        self.controlSocket = controlSocket
        self.controlCapability = controlCapability
    }

    public static func live(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) throws -> Self {
        let stateDirectory: URL
        if let override = environment["NONPROXY_STATE_DIR"],
           override.hasPrefix("/")
        {
            stateDirectory = URL(
                fileURLWithPath: override,
                isDirectory: true
            )
        } else {
            guard let home = environment["HOME"],
                  home.hasPrefix("/")
            else {
                throw NativeMessagingError.runtimeUnavailable(
                    "无法定位 NonProxy 状态目录。"
                )
            }
            stateDirectory = URL(
                fileURLWithPath: home,
                isDirectory: true
            )
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent(
                "Group Containers",
                isDirectory: true
            )
            .appendingPathComponent(
                "group.com.nonproxy.shared",
                isDirectory: true
            )
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent(
                "Application Support",
                isDirectory: true
            )
            .appendingPathComponent("NonProxy", isDirectory: true)
        }
        return try Self(
            controlSocket: stateDirectory
                .appendingPathComponent("gatewayd.sock"),
            controlCapability: stateDirectory
                .appendingPathComponent("session.capability")
        )
    }

    public static func appExtension(
        appGroupContainer: URL?
    ) throws -> Self {
        guard let appGroupContainer,
              appGroupContainer.isFileURL,
              appGroupContainer.path.hasPrefix("/")
        else {
            throw NativeMessagingError.runtimeUnavailable(
                "无法访问 NonProxy App Group 状态目录。"
            )
        }
        let stateDirectory = appGroupContainer
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent(
                "Application Support",
                isDirectory: true
            )
            .appendingPathComponent("NonProxy", isDirectory: true)
        return try Self(
            controlSocket: stateDirectory
                .appendingPathComponent("gatewayd.sock"),
            controlCapability: stateDirectory
                .appendingPathComponent("session.capability")
        )
    }

    public func readControlCapability() throws -> Data {
        let descriptor = open(
            controlCapability.path,
            O_RDONLY | O_CLOEXEC | O_NOFOLLOW
        )
        guard descriptor >= 0 else {
            throw NativeMessagingError.runtimeUnavailable(
                "无法读取本地控制能力文件。"
            )
        }
        defer {
            close(descriptor)
        }

        var status = stat()
        guard fstat(descriptor, &status) == 0,
              status.st_uid == geteuid(),
              status.st_mode & S_IFMT == S_IFREG,
              status.st_mode & 0o077 == 0,
              status.st_size == 32
        else {
            throw NativeMessagingError.runtimeUnavailable(
                "本地控制能力文件权限或类型无效。"
            )
        }

        var bytes = [UInt8](repeating: 0, count: 32)
        var offset = 0
        while offset < bytes.count {
            let remaining = bytes.count - offset
            let count = bytes.withUnsafeMutableBytes { buffer in
                read(
                    descriptor,
                    buffer.baseAddress?.advanced(by: offset),
                    remaining
                )
            }
            guard count > 0 else {
                throw NativeMessagingError.runtimeUnavailable(
                    "本地控制能力文件内容不完整。"
                )
            }
            offset += count
        }
        return Data(bytes)
    }

    public func validateControlSocket() throws {
        var status = stat()
        guard lstat(controlSocket.path, &status) == 0,
              status.st_uid == geteuid(),
              status.st_mode & S_IFMT == S_IFSOCK,
              status.st_mode & 0o077 == 0
        else {
            throw NativeMessagingError.runtimeUnavailable(
                "本地控制套接字权限或类型无效。"
            )
        }
    }
}
