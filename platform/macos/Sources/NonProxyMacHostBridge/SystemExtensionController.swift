import Foundation
import SystemExtensions

@MainActor
struct SystemExtensionController {
    func query(bundleIdentifier: String) async throws
        -> SystemExtensionSnapshot
    {
        let matches = try await SystemExtensionPropertiesRequest()
            .execute(bundleIdentifier: bundleIdentifier)
        guard !matches.isEmpty else {
            return SystemExtensionSnapshot(
                bundleIdentifier: bundleIdentifier,
                installed: false,
                enabled: false,
                awaitingUserApproval: false,
                uninstalling: false,
                bundleVersion: nil,
                bundleShortVersion: nil
            )
        }
        guard matches.count == 1, let match = matches.first else {
            throw BridgeError(
                code: "NP_MAC_SYSTEM_EXTENSION_AMBIGUOUS",
                message: "系统中存在多个相同标识的扩展，无法安全判断状态。"
            )
        }
        return match
    }

    func activate(
        bundleIdentifier: String,
        approvalHandler: @escaping () -> Void
    ) async throws -> SystemExtensionMutationOutcome {
        try await SystemExtensionMutationRequest().execute(
            bundleIdentifier: bundleIdentifier,
            action: .activate,
            approvalHandler: approvalHandler
        )
    }

    func deactivate(bundleIdentifier: String) async throws
        -> SystemExtensionMutationOutcome
    {
        do {
            return try await SystemExtensionMutationRequest().execute(
                bundleIdentifier: bundleIdentifier,
                action: .deactivate,
                approvalHandler: {}
            )
        } catch {
            let nsError = error as NSError
            if nsError.domain == OSSystemExtensionError.errorDomain,
               nsError.code
                   == OSSystemExtensionError.Code.extensionNotFound.rawValue
            {
                return SystemExtensionMutationOutcome(requiresReboot: false)
            }
            throw error
        }
    }
}
