import Foundation
import NonProxyMacNetworkIdentity
import SystemExtensions

enum BridgeConstants {
    static let abiVersion: UInt32 = 7
    static let transparentBundleIdentifier =
        "com.nonproxy.desktop.transparent-proxy"
    static let dnsBundleIdentifier = "com.nonproxy.desktop.dns-proxy"
    static let localizedDescription = "NonProxy 智能分流"
}

enum NetworkLocationPermissionState: String, Codable, Sendable {
    case authorized
    case denied
    case restricted
    case notDetermined
    case unknown
}

struct NetworkFingerprintDescriptor: Codable, Equatable, Sendable {
    let kind: MacNetworkFingerprintKind
    let value: String
}

struct CurrentNetworkPayload: Codable, Equatable, Sendable {
    let success: Bool
    let message: String
    let errorCode: String?
    let permissionState: NetworkLocationPermissionState
    let suggestedName: String?
    let fingerprint: NetworkFingerprintDescriptor?

    static func result(
        snapshot: MacNetworkEnvironmentSnapshot,
        permission: NetworkLocationPermissionState
    ) -> CurrentNetworkPayload {
        result(
            fingerprints: snapshot.fingerprints,
            permission: permission
        )
    }

    static func result(
        fingerprints: [MacNetworkFingerprint],
        permission: NetworkLocationPermissionState
    ) -> CurrentNetworkPayload {
        guard let fingerprint = fingerprints.first else {
            return CurrentNetworkPayload(
                success: false,
                message: "当前没有可识别的物理网络，请连接网络后重试。",
                errorCode: "NP_MAC_NETWORK_UNAVAILABLE",
                permissionState: permission,
                suggestedName: nil,
                fingerprint: nil
            )
        }
        let suggestedName: String
        let message: String
        switch fingerprint.kind {
        case .wifiSSIDHash:
            suggestedName = "当前 Wi-Fi"
            message = "已用本机哈希识别当前 Wi-Fi；原始网络名称不会离开原生采集栈。"
        case .defaultGatewayHash:
            suggestedName = "当前局域网"
            message = permission == .denied || permission == .restricted
                ? "定位权限不可用，已改用物理网关的脱敏指纹。"
                : "已用物理网关的脱敏指纹识别当前网络。"
        case .interfaceClass:
            suggestedName = fingerprint.value == "ethernet"
                ? "当前有线网络"
                : "当前网络"
            message = "只能识别当前网络类型；在同类型网络间切换时可能共用此配置。"
        }
        return CurrentNetworkPayload(
            success: true,
            message: message,
            errorCode: nil,
            permissionState: permission,
            suggestedName: suggestedName,
            fingerprint: NetworkFingerprintDescriptor(
                kind: fingerprint.kind,
                value: fingerprint.value
            )
        )
    }
}

struct ApplicationDescriptor: Codable, Equatable, Sendable {
    let displayName: String
    let stableIdentity: String
    let signerIdentity: String?
    let bundleIdentifier: String?
    let isRunning: Bool
}

struct ApplicationCatalogPayload: Codable, Equatable, Sendable {
    let success: Bool
    let message: String
    let errorCode: String?
    let applications: [ApplicationDescriptor]

    static func result(
        applications: [ApplicationDescriptor]
    ) -> ApplicationCatalogPayload {
        ApplicationCatalogPayload(
            success: true,
            message: applications.isEmpty
                ? "没有发现可选择的应用；可从“应用程序”文件夹手动选择。"
                : "选择一个应用，即可让它的全部网络请求直连。",
            errorCode: nil,
            applications: applications
        )
    }

    static func failure(error: Error) -> ApplicationCatalogPayload {
        let bridgeError = BridgeError.from(error)
        return ApplicationCatalogPayload(
            success: false,
            message: bridgeError.message,
            errorCode: bridgeError.code,
            applications: []
        )
    }
}

struct ApplicationSelectionPayload: Codable, Equatable, Sendable {
    let success: Bool
    let message: String
    let errorCode: String?
    let application: ApplicationDescriptor?

    static func result(
        application: ApplicationDescriptor?
    ) -> ApplicationSelectionPayload {
        ApplicationSelectionPayload(
            success: true,
            message: application == nil ? "未选择应用。" : "已读取所选应用身份。",
            errorCode: nil,
            application: application
        )
    }

    static func failure(error: Error) -> ApplicationSelectionPayload {
        let bridgeError = BridgeError.from(error)
        return ApplicationSelectionPayload(
            success: false,
            message: bridgeError.message,
            errorCode: bridgeError.code,
            application: nil
        )
    }
}

struct SystemProxyDescriptor: Codable, Equatable, Sendable {
    let suggestedID: String
    let displayName: String
    let kind: String
    let host: String
    let port: UInt16
}

struct SystemProxyDiscoveryPayload: Codable, Equatable, Sendable {
    let success: Bool
    let message: String
    let errorCode: String?
    let proxies: [SystemProxyDescriptor]

    static func result(
        proxies: [SystemProxyDescriptor]
    ) -> SystemProxyDiscoveryPayload {
        SystemProxyDiscoveryPayload(
            success: true,
            message: proxies.isEmpty
                ? "系统当前没有启用可导入的 SOCKS 或 HTTP 代理。"
                : "已从系统设置发现 \(proxies.count) 个代理；格式检查不代表握手可用，请核对后再保存和测试。",
            errorCode: nil,
            proxies: proxies
        )
    }

    static func failure(error: Error) -> SystemProxyDiscoveryPayload {
        let bridgeError = BridgeError.from(error)
        return SystemProxyDiscoveryPayload(
            success: false,
            message: bridgeError.message,
            errorCode: bridgeError.code,
            proxies: []
        )
    }
}

enum BridgeOperation: String, Codable, Sendable {
    case probe
    case query
    case installAndEnable
    case disableAndUninstall
}

struct SystemExtensionSnapshot: Codable, Equatable, Sendable {
    let bundleIdentifier: String
    let installed: Bool
    let enabled: Bool
    let awaitingUserApproval: Bool
    let uninstalling: Bool
    let bundleVersion: String?
    let bundleShortVersion: String?
}

struct NetworkPreferenceSnapshot: Codable, Equatable, Sendable {
    let configured: Bool
    let enabled: Bool
}

struct GatewayAgentSnapshot: Codable, Equatable, Sendable {
    let registered: Bool
    let enabled: Bool
    let requiresApproval: Bool
    let found: Bool
    let ready: Bool
    let requiresUpgrade: Bool
}

struct MacHostState: Codable, Equatable, Sendable {
    let gatewayAgent: GatewayAgentSnapshot
    let transparentExtension: SystemExtensionSnapshot
    let dnsExtension: SystemExtensionSnapshot
    let transparentPreference: NetworkPreferenceSnapshot
    let dnsPreference: NetworkPreferenceSnapshot
}

struct BridgeEventPayload: Codable, Equatable, Sendable {
    let operation: BridgeOperation
    let success: Bool
    let message: String
    let errorCode: String?
    let requiresReboot: Bool
    let state: MacHostState?

    static func success(
        operation: BridgeOperation,
        message: String,
        requiresReboot: Bool = false,
        state: MacHostState? = nil
    ) -> BridgeEventPayload {
        BridgeEventPayload(
            operation: operation,
            success: true,
            message: message,
            errorCode: nil,
            requiresReboot: requiresReboot,
            state: state
        )
    }

    static func failure(
        operation: BridgeOperation,
        error: Error
    ) -> BridgeEventPayload {
        let bridgeError = BridgeError.from(error)
        return BridgeEventPayload(
            operation: operation,
            success: false,
            message: bridgeError.message,
            errorCode: bridgeError.code,
            requiresReboot: false,
            state: nil
        )
    }
}

struct ProbePayload: Codable, Equatable, Sendable {
    let abiVersion: UInt32
    let message: String
}

struct BridgeError: Error, Equatable, Sendable {
    let code: String
    let message: String

    static func from(_ error: Error) -> BridgeError {
        if let bridgeError = error as? BridgeError {
            return bridgeError
        }
        let nsError = error as NSError
        if nsError.domain == OSSystemExtensionError.errorDomain {
            switch nsError.code {
            case OSSystemExtensionError.Code.missingEntitlement.rawValue:
                return BridgeError(
                    code: "NP_MAC_MISSING_ENTITLEMENT",
                    message: "当前应用签名缺少安装系统扩展所需的权限。"
                )
            case OSSystemExtensionError.Code
                .unsupportedParentBundleLocation.rawValue:
                return BridgeError(
                    code: "NP_MAC_UNSUPPORTED_BUNDLE_LOCATION",
                    message: "请把 NonProxy 移到“应用程序”目录后重试。"
                )
            case OSSystemExtensionError.Code
                .authorizationRequired.rawValue:
                return BridgeError(
                    code: "NP_MAC_AUTHORIZATION_REQUIRED",
                    message: "macOS 需要管理员授权才能完成此操作。"
                )
            case OSSystemExtensionError.Code
                .forbiddenBySystemPolicy.rawValue:
                return BridgeError(
                    code: "NP_MAC_FORBIDDEN_BY_SYSTEM_POLICY",
                    message: "系统策略禁止启用 NonProxy 网络扩展。"
                )
            default:
                break
            }
        }
        return BridgeError(
            code: "NP_MAC_NATIVE_ERROR_\(nsError.code)",
            message: nsError.localizedDescription
        )
    }
}

struct SystemExtensionMutationOutcome: Equatable, Sendable {
    let requiresReboot: Bool
}

struct GatewayAgentRegistrationOutcome: Equatable, Sendable {
    let newlyRegistered: Bool
}
