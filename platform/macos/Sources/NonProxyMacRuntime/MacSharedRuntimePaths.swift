import Foundation

public struct MacSharedRuntimePaths: Equatable, Sendable {
    public static let appGroupIdentifier = "group.com.nonproxy.shared"
    public static let gatewayAgentPlistName =
        "com.nonproxy.gatewayd.plist"
    public static let controlSocketFileName = "gatewayd.sock"
    public static let flowSocketFileName = "gatewayd-flow.sock"
    public static let controlCapabilityFileName = "session.capability"
    public static let providerCapabilityFileName = "provider.capability"

    public let stateDirectory: URL

    public init(stateDirectory: URL) throws {
        guard stateDirectory.isFileURL,
              stateDirectory.path.hasPrefix("/")
        else {
            throw MacRuntimePathError.invalidStateDirectory
        }
        self.stateDirectory = stateDirectory
    }

    public static func live(
        fileManager: FileManager = .default
    ) throws -> MacSharedRuntimePaths {
        guard let container = fileManager.containerURL(
            forSecurityApplicationGroupIdentifier: appGroupIdentifier
        ) else {
            throw MacRuntimePathError.appGroupUnavailable
        }
        return try MacSharedRuntimePaths(
            stateDirectory: container
                .appendingPathComponent("Library", isDirectory: true)
                .appendingPathComponent(
                    "Application Support",
                    isDirectory: true
                )
                .appendingPathComponent("NonProxy", isDirectory: true)
        )
    }

    public var controlSocket: URL {
        stateDirectory.appendingPathComponent(
            Self.controlSocketFileName
        )
    }

    public var flowSocket: URL {
        stateDirectory.appendingPathComponent(Self.flowSocketFileName)
    }

    public var controlCapability: URL {
        stateDirectory.appendingPathComponent(
            Self.controlCapabilityFileName
        )
    }

    public var providerCapability: URL {
        stateDirectory.appendingPathComponent(
            Self.providerCapabilityFileName
        )
    }

    public var providerCacheDirectory: URL {
        stateDirectory.appendingPathComponent(
            "provider-cache",
            isDirectory: true
        )
    }
}

public enum MacRuntimePathError: Error, Equatable, Sendable {
    case invalidStateDirectory
    case appGroupUnavailable
}
