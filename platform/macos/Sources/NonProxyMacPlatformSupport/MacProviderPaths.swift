import Foundation
import NonProxyProviderCore

public struct MacProviderPaths: Sendable {
    public static let appGroupIdentifier = "group.com.nonproxy.shared"

    public let stateDirectory: URL

    public init(stateDirectory: URL) throws {
        guard stateDirectory.isFileURL,
              stateDirectory.path.hasPrefix("/")
        else {
            throw ProviderError.invalidConfiguration("Provider 状态目录必须为绝对文件路径")
        }
        self.stateDirectory = stateDirectory
    }

    public static func live(
        fileManager: FileManager = .default
    ) throws -> Self {
        guard let container = fileManager.containerURL(
            forSecurityApplicationGroupIdentifier: appGroupIdentifier
        ) else {
            throw ProviderError.invalidConfiguration("Provider 无法访问共享 App Group")
        }
        return try Self(
            stateDirectory: container
                .appendingPathComponent("Library", isDirectory: true)
                .appendingPathComponent("Application Support", isDirectory: true)
                .appendingPathComponent("NonProxy", isDirectory: true)
        )
    }

    public var socketPath: String {
        stateDirectory.appendingPathComponent("gatewayd.sock").path
    }

    public var flowSocketPath: String {
        stateDirectory.appendingPathComponent("gatewayd-flow.sock").path
    }

    public var cacheDirectory: URL {
        stateDirectory.appendingPathComponent("provider-cache", isDirectory: true)
    }

    public func readBootstrapCapability(
        fileManager: FileManager = .default
    ) throws -> Data {
        let directoryValues = try stateDirectory.resourceValues(
            forKeys: [.isDirectoryKey, .isSymbolicLinkKey]
        )
        guard directoryValues.isDirectory == true,
              directoryValues.isSymbolicLink != true
        else {
            throw ProviderError.invalidConfiguration("Provider 状态目录类型无效")
        }
        let file = stateDirectory.appendingPathComponent("provider.capability")
        let values = try file.resourceValues(
            forKeys: [.isRegularFileKey, .isSymbolicLinkKey]
        )
        guard values.isRegularFile == true, values.isSymbolicLink != true else {
            throw ProviderError.invalidConfiguration("Provider 启动凭据文件类型无效")
        }
        let attributes = try fileManager.attributesOfItem(atPath: file.path)
        let directoryAttributes = try fileManager.attributesOfItem(
            atPath: stateDirectory.path
        )
        guard let directoryPermissions =
                directoryAttributes[.posixPermissions] as? NSNumber,
              directoryPermissions.intValue & 0o077 == 0,
              let permissions = attributes[.posixPermissions] as? NSNumber,
              permissions.intValue & 0o077 == 0,
              let owner = attributes[.ownerAccountID] as? NSNumber,
              let directoryOwner = directoryAttributes[.ownerAccountID] as? NSNumber,
              owner == directoryOwner
        else {
            throw ProviderError.invalidConfiguration("Provider 启动凭据文件权限无效")
        }
        let capability = try Data(contentsOf: file, options: [.uncached])
        guard capability.count == 32 else {
            throw ProviderError.invalidConfiguration("Provider 启动凭据长度无效")
        }
        return capability
    }
}
