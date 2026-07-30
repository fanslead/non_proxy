import Foundation
import NonProxyMacRuntime
import ServiceManagement

@MainActor
struct GatewayAgentController {
    private static let readinessAttempts = 100
    private static let readinessDelay = Duration.milliseconds(100)

    private let service: SMAppService

    init(
        service: SMAppService = .agent(
            plistName: MacSharedRuntimePaths.gatewayAgentPlistName
        )
    ) {
        self.service = service
    }

    func query() -> GatewayAgentSnapshot {
        let status = service.status
        let runtimeReady =
            status == .enabled && isRuntimeReady()
        return Self.snapshot(
            status: status,
            runtimeReady: runtimeReady
        )
    }

    func registerAndWait(
        approvalHandler: @escaping () -> Void
    ) async throws -> GatewayAgentRegistrationOutcome {
        try requireAppGroupContainer()
        let initialStatus = service.status
        switch initialStatus {
        case .notFound:
            throw Self.notPackagedError()
        case .requiresApproval:
            approvalHandler()
            throw Self.approvalRequiredError()
        case .enabled:
            try await waitUntilReady()
            return GatewayAgentRegistrationOutcome(
                newlyRegistered: false
            )
        case .notRegistered:
            break
        @unknown default:
            throw Self.unknownStatusError()
        }

        var didRegister = false
        do {
            try service.register()
            didRegister = true
        } catch {
            let currentStatus = service.status
            if currentStatus == .requiresApproval {
                approvalHandler()
                throw Self.approvalRequiredError()
            }
            if currentStatus != .enabled {
                throw Self.mapRegistrationError(error)
            }
        }

        switch service.status {
        case .enabled:
            do {
                try await waitUntilReady()
            } catch {
                guard didRegister else {
                    throw error
                }
                do {
                    try await unregister()
                } catch let rollbackError {
                    throw BridgeError(
                        code: "NP_MAC_INSTALL_ROLLBACK_FAILED",
                        message:
                            "\(error.localizedDescription)；gatewayd 回滚失败："
                            + rollbackError.localizedDescription
                    )
                }
                throw error
            }
            return GatewayAgentRegistrationOutcome(
                newlyRegistered: didRegister
            )
        case .requiresApproval:
            approvalHandler()
            throw Self.approvalRequiredError()
        case .notFound:
            throw Self.notPackagedError()
        case .notRegistered:
            throw BridgeError(
                code: "NP_MAC_GATEWAY_REGISTRATION_FAILED",
                message: "gatewayd 未能登记为用户后台项目。"
            )
        @unknown default:
            throw Self.unknownStatusError()
        }
    }

    func unregister() async throws {
        switch service.status {
        case .notRegistered:
            return
        case .notFound:
            throw Self.notPackagedError()
        case .enabled, .requiresApproval:
            break
        @unknown default:
            throw Self.unknownStatusError()
        }

        do {
            try await service.unregister()
        } catch {
            let nsError = error as NSError
            if nsError.domain == SMAppServiceErrorDomain,
               nsError.code == kSMErrorJobNotFound
            {
                return
            }
            throw BridgeError(
                code: "NP_MAC_GATEWAY_UNREGISTER_FAILED",
                message: "无法停止并移除 gatewayd 后台项目："
                    + nsError.localizedDescription
            )
        }
    }

    static func snapshot(
        status: SMAppService.Status,
        runtimeReady: Bool
    ) -> GatewayAgentSnapshot {
        switch status {
        case .notRegistered:
            return GatewayAgentSnapshot(
                registered: false,
                enabled: false,
                requiresApproval: false,
                found: true,
                ready: false
            )
        case .enabled:
            return GatewayAgentSnapshot(
                registered: true,
                enabled: true,
                requiresApproval: false,
                found: true,
                ready: runtimeReady
            )
        case .requiresApproval:
            return GatewayAgentSnapshot(
                registered: true,
                enabled: false,
                requiresApproval: true,
                found: true,
                ready: false
            )
        case .notFound:
            return GatewayAgentSnapshot(
                registered: false,
                enabled: false,
                requiresApproval: false,
                found: false,
                ready: false
            )
        @unknown default:
            return GatewayAgentSnapshot(
                registered: false,
                enabled: false,
                requiresApproval: false,
                found: false,
                ready: false
            )
        }
    }

    private func waitUntilReady() async throws {
        for attempt in 0..<Self.readinessAttempts {
            if isRuntimeReady() {
                return
            }
            if attempt + 1 < Self.readinessAttempts {
                try await Task.sleep(for: Self.readinessDelay)
            }
        }
        throw BridgeError(
            code: "NP_MAC_GATEWAY_NOT_READY",
            message: "gatewayd 已获准运行，但本地控制通道未在限定时间内就绪。"
        )
    }

    private func isRuntimeReady() -> Bool {
        guard let paths = try? MacSharedRuntimePaths.live() else {
            return false
        }
        do {
            try GatewayRuntimeReadiness.inspect(paths: paths)
            return true
        } catch {
            return false
        }
    }

    private func requireAppGroupContainer() throws {
        do {
            _ = try MacSharedRuntimePaths.live()
        } catch {
            throw BridgeError(
                code: "NP_MAC_APP_GROUP_UNAVAILABLE",
                message: "当前应用无法访问 NonProxy 共享 App Group，请检查签名与权限。"
            )
        }
    }

    private static func mapRegistrationError(
        _ error: Error
    ) -> BridgeError {
        let nsError = error as NSError
        guard nsError.domain == SMAppServiceErrorDomain else {
            return BridgeError(
                code: "NP_MAC_GATEWAY_REGISTRATION_FAILED",
                message: "无法登记 gatewayd 后台项目："
                    + nsError.localizedDescription
            )
        }
        switch nsError.code {
        case kSMErrorInvalidSignature:
            return BridgeError(
                code: "NP_MAC_GATEWAY_INVALID_SIGNATURE",
                message: "gatewayd 或宿主应用的代码签名无效。"
            )
        case kSMErrorJobPlistNotFound,
             kSMErrorToolNotValid:
            return notPackagedError()
        case kSMErrorLaunchDeniedByUser:
            return approvalRequiredError()
        default:
            return BridgeError(
                code: "NP_MAC_GATEWAY_REGISTRATION_FAILED",
                message: "无法登记 gatewayd 后台项目："
                    + nsError.localizedDescription
            )
        }
    }

    private static func approvalRequiredError() -> BridgeError {
        BridgeError(
            code: "NP_MAC_GATEWAY_APPROVAL_REQUIRED",
            message: "请在“系统设置 → 通用 → 登录项与扩展”中允许 NonProxy 后台项目，然后重试。"
        )
    }

    private static func notPackagedError() -> BridgeError {
        BridgeError(
            code: "NP_MAC_GATEWAY_NOT_PACKAGED",
            message: "当前 NonProxy 安装包缺少 gatewayd 后台项目。"
        )
    }

    private static func unknownStatusError() -> BridgeError {
        BridgeError(
            code: "NP_MAC_GATEWAY_STATUS_UNKNOWN",
            message: "macOS 返回了无法识别的 gatewayd 后台项目状态。"
        )
    }
}
