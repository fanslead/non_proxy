import Foundation
import SystemExtensions

enum BridgeConstants {
    static let abiVersion: UInt32 = 4
    static let transparentBundleIdentifier =
        "com.nonproxy.desktop.transparent-proxy"
    static let dnsBundleIdentifier = "com.nonproxy.desktop.dns-proxy"
    static let localizedDescription = "NonProxy 智能分流"
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
