import Foundation
import NonProxyProviderContracts

public struct ProviderConfiguration: Sendable {
    public static let protocolMajor: UInt32 = 1
    public static let protocolMinor: UInt32 = 0

    public let kind: Nonproxy_Provider_V1_ProviderKind
    public let component: Nonproxy_Common_V1_ComponentKind
    public let socketPath: String
    public let bootstrapCapability: Data
    public let cacheDirectory: URL
    public let semanticVersion: String
    public let buildID: String

    public init(
        kind: Nonproxy_Provider_V1_ProviderKind,
        component: Nonproxy_Common_V1_ComponentKind,
        socketPath: String,
        bootstrapCapability: Data,
        cacheDirectory: URL,
        semanticVersion: String,
        buildID: String
    ) throws {
        let validComponent = (kind == .transparentProxy && component == .transparentProxy)
            || (kind == .dnsProxy && component == .dnsProxy)
        guard validComponent,
              socketPath.hasPrefix("/"),
              !socketPath.contains("\0"),
              !bootstrapCapability.isEmpty,
              !semanticVersion.isEmpty,
              !buildID.isEmpty
        else {
            throw ProviderError.invalidConfiguration("Provider 启动配置不完整")
        }
        guard bootstrapCapability.count == 32 else {
            throw ProviderError.invalidConfiguration("Provider 启动凭据必须为 32 字节")
        }

        self.kind = kind
        self.component = component
        self.socketPath = socketPath
        self.bootstrapCapability = bootstrapCapability
        self.cacheDirectory = cacheDirectory
        self.semanticVersion = semanticVersion
        self.buildID = buildID
    }

    public var componentVersion: Nonproxy_Common_V1_ComponentVersion {
        var version = Nonproxy_Common_V1_ComponentVersion()
        version.component = component
        version.semanticVersion = semanticVersion
        version.buildID = buildID
        version.protocolMajor = Self.protocolMajor
        version.protocolMinor = Self.protocolMinor
        version.minimumProtocolMinor = Self.protocolMinor
        return version
    }
}
