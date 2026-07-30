import Foundation

@MainActor
enum NetworkPreferenceTransaction {
    static func enable(
        applyTransparent: () async throws -> Void,
        applyDNS: () async throws -> Void,
        restoreDNS: () async throws -> Void,
        restoreTransparent: () async throws -> Void
    ) async throws {
        var transparentAttempted = false
        var dnsAttempted = false
        do {
            transparentAttempted = true
            try await applyTransparent()
            dnsAttempted = true
            try await applyDNS()
        } catch {
            var rollbackErrors: [String] = []
            if dnsAttempted {
                do {
                    try await restoreDNS()
                } catch {
                    rollbackErrors.append(
                        "DNS 配置恢复失败：\(error.localizedDescription)"
                    )
                }
            }
            if transparentAttempted {
                do {
                    try await restoreTransparent()
                } catch {
                    rollbackErrors.append(
                        "透明代理配置恢复失败：\(error.localizedDescription)"
                    )
                }
            }
            guard rollbackErrors.isEmpty else {
                throw BridgeError(
                    code: "NP_MAC_PREFERENCE_ROLLBACK_FAILED",
                    message:
                        "网络配置失败，且旧配置未能完整恢复："
                        + rollbackErrors.joined(separator: "；")
                )
            }
            throw error
        }
    }
}
