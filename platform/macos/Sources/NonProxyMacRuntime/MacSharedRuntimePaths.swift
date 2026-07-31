import Foundation

public struct MacSharedRuntimePaths: Equatable, Sendable {
    public static let appGroupIdentifier = "group.com.nonproxy.shared"
    public static let gatewayAgentPlistName =
        "com.nonproxy.gatewayd.plist"
    public static let adapterHostAgentPlistName =
        "com.nonproxy.adapter-host.plist"
    public static let controlSocketFileName = "gatewayd.sock"
    public static let flowSocketFileName = "gatewayd-flow.sock"
    public static let controlCapabilityFileName = "session.capability"
    public static let providerCapabilityFileName = "provider.capability"
    public static let runtimeIdentityFileName = "gateway.runtime.json"
    public static let adapterHostDirectoryName = "adapter-host"
    public static let adapterHostSocketFileName = "adapter-host.sock"
    public static let adapterHostCapabilityFileName = "adapter.capability"
    public static let adapterHostRuntimeIdentityFileName =
        "adapter.runtime.json"
    public static let nativeMessagingHostFileName =
        "nonproxy-native-messaging-host"
    public static let chromiumExtensionID =
        "ldiadofihjimpkhchjicmgcfgjlgidha"

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

    public var runtimeIdentity: URL {
        stateDirectory.appendingPathComponent(
            Self.runtimeIdentityFileName
        )
    }

    public var adapterHostStateDirectory: URL {
        stateDirectory.appendingPathComponent(
            Self.adapterHostDirectoryName,
            isDirectory: true
        )
    }

    public var adapterHostSocket: URL {
        adapterHostStateDirectory.appendingPathComponent(
            Self.adapterHostSocketFileName
        )
    }

    public var adapterHostCapability: URL {
        adapterHostStateDirectory.appendingPathComponent(
            Self.adapterHostCapabilityFileName
        )
    }

    public var adapterHostRuntimeIdentity: URL {
        adapterHostStateDirectory.appendingPathComponent(
            Self.adapterHostRuntimeIdentityFileName
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
