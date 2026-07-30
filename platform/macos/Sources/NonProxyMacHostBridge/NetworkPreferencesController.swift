@preconcurrency import NetworkExtension
import Foundation

@MainActor
struct NetworkPreferencesController {
    func query() async throws
        -> (
            transparent: NetworkPreferenceSnapshot,
            dns: NetworkPreferenceSnapshot
        )
    {
        let transparentManager = try await loadTransparentManager()
        let dnsManager = try await loadDNSManager()
        return (
            preferenceSnapshot(transparentManager),
            preferenceSnapshot(dnsManager)
        )
    }

    func enable() async throws {
        let existingTransparent = try await loadTransparentManager()
        let transparentManager =
            existingTransparent ?? NETransparentProxyManager()
        let transparentSnapshot = TransparentPreferenceBackup(
            existed: existingTransparent != nil,
            localizedDescription: transparentManager.localizedDescription,
            protocolConfiguration: try copyProtocol(
                transparentManager.protocolConfiguration
            ),
            enabled: transparentManager.isEnabled
        )

        let dnsManager = try await loadDNSManager()
        let dnsSnapshot = DNSPreferenceBackup(
            configured: dnsManager.providerProtocol != nil,
            localizedDescription: dnsManager.localizedDescription,
            providerProtocol: try copyProtocol(
                dnsManager.providerProtocol
            ),
            enabled: dnsManager.isEnabled
        )
        if let provider = dnsManager.providerProtocol,
           provider.providerBundleIdentifier
               != BridgeConstants.dnsBundleIdentifier
        {
            throw BridgeError(
                code: "NP_MAC_DNS_PREFERENCE_CONFLICT",
                message: "当前应用名下存在其他 DNS 代理配置，NonProxy 不会覆盖它。"
            )
        }

        try await NetworkPreferenceTransaction.enable(
            applyTransparent: {
                configureTransparent(transparentManager)
                try await save(transparentManager)
            },
            applyDNS: {
                configureDNS(dnsManager)
                try await save(dnsManager)
            },
            restoreDNS: {
                try await restore(dnsManager, from: dnsSnapshot)
            },
            restoreTransparent: {
                try await restore(
                    transparentManager,
                    from: transparentSnapshot
                )
            }
        )
    }

    func disableAndRemove() async throws {
        var errors: [String] = []

        do {
            if let transparentManager = try await loadTransparentManager() {
                transparentManager.isEnabled = false
                try await save(transparentManager)
                try await remove(transparentManager)
            }
        } catch {
            errors.append("透明代理：\(error.localizedDescription)")
        }

        do {
            let dnsManager = try await loadDNSManager()
            if dnsManager.providerProtocol?.providerBundleIdentifier
                == BridgeConstants.dnsBundleIdentifier
            {
                dnsManager.isEnabled = false
                try await save(dnsManager)
                try await remove(dnsManager)
            }
        } catch {
            errors.append("DNS 代理：\(error.localizedDescription)")
        }

        guard errors.isEmpty else {
            throw BridgeError(
                code: "NP_MAC_PREFERENCE_REMOVE_FAILED",
                message: "部分网络配置未能移除：" + errors.joined(separator: "；")
            )
        }
    }

    private func loadTransparentManager() async throws
        -> NETransparentProxyManager?
    {
        let result = try await withCheckedThrowingContinuation {
            (continuation:
                CheckedContinuation<TransparentManagerList, Error>) in
            NETransparentProxyManager.loadAllFromPreferences {
                managers,
                error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: TransparentManagerList(
                        values: managers ?? []
                    ))
                }
            }
        }
        let matches = result.values.filter { manager in
            let provider =
                manager.protocolConfiguration as? NETunnelProviderProtocol
            return provider?.providerBundleIdentifier
                == BridgeConstants.transparentBundleIdentifier
        }
        guard matches.count <= 1 else {
            throw BridgeError(
                code: "NP_MAC_TRANSPARENT_PREFERENCE_AMBIGUOUS",
                message: "检测到多个 NonProxy 透明代理配置，已停止以防误改。"
            )
        }
        return matches.first
    }

    private func loadDNSManager() async throws -> NEDNSProxyManager {
        let manager = NEDNSProxyManager.shared()
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            manager.loadFromPreferences { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: ())
                }
            }
        }
        return manager
    }

    private func configureTransparent(
        _ manager: NETransparentProxyManager
    ) {
        let provider = NETunnelProviderProtocol()
        provider.providerBundleIdentifier =
            BridgeConstants.transparentBundleIdentifier
        // Apple 要求协议对象具有非空 serverAddress；透明代理不会连接该地址。
        provider.serverAddress = "localhost"
        provider.providerConfiguration = [
            "schemaVersion": 1,
            "configurationOwner": "NonProxy",
        ]
        manager.protocolConfiguration = provider
        manager.localizedDescription = BridgeConstants.localizedDescription
        manager.isEnabled = true
    }

    private func configureDNS(_ manager: NEDNSProxyManager) {
        let provider = NEDNSProxyProviderProtocol()
        provider.providerBundleIdentifier = BridgeConstants.dnsBundleIdentifier
        provider.providerConfiguration = [
            "schemaVersion": 1,
            "configurationOwner": "NonProxy",
        ]
        manager.providerProtocol = provider
        manager.localizedDescription = BridgeConstants.localizedDescription
        manager.isEnabled = true
    }

    private func copyProtocol(_ protocolValue: NEVPNProtocol?) throws
        -> NEVPNProtocol?
    {
        guard let protocolValue else {
            return nil
        }
        guard let copy = protocolValue.copy() as? NEVPNProtocol else {
            throw BridgeError(
                code: "NP_MAC_PREFERENCE_SNAPSHOT_FAILED",
                message: "无法复制当前透明代理配置，已停止以防覆盖旧配置。"
            )
        }
        return copy
    }

    private func copyProtocol(
        _ protocolValue: NEDNSProxyProviderProtocol?
    ) throws -> NEDNSProxyProviderProtocol? {
        guard let protocolValue else {
            return nil
        }
        guard let copy =
            protocolValue.copy() as? NEDNSProxyProviderProtocol
        else {
            throw BridgeError(
                code: "NP_MAC_PREFERENCE_SNAPSHOT_FAILED",
                message: "无法复制当前 DNS 代理配置，已停止以防覆盖旧配置。"
            )
        }
        return copy
    }

    private func preferenceSnapshot(
        _ manager: NETransparentProxyManager?
    ) -> NetworkPreferenceSnapshot {
        NetworkPreferenceSnapshot(
            configured: manager != nil,
            enabled: manager?.isEnabled ?? false
        )
    }

    private func preferenceSnapshot(
        _ manager: NEDNSProxyManager
    ) -> NetworkPreferenceSnapshot {
        let configured = manager.providerProtocol?.providerBundleIdentifier
            == BridgeConstants.dnsBundleIdentifier
        return NetworkPreferenceSnapshot(
            configured: configured,
            enabled: configured && manager.isEnabled
        )
    }

    private func save(_ manager: NETransparentProxyManager) async throws {
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            manager.saveToPreferences { error in
                Self.resume(continuation, error: error)
            }
        }
    }

    private func remove(_ manager: NETransparentProxyManager) async throws {
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            manager.removeFromPreferences { error in
                Self.resume(continuation, error: error)
            }
        }
    }

    private func save(_ manager: NEDNSProxyManager) async throws {
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            manager.saveToPreferences { error in
                Self.resume(continuation, error: error)
            }
        }
    }

    private func remove(_ manager: NEDNSProxyManager) async throws {
        try await withCheckedThrowingContinuation {
            (continuation: CheckedContinuation<Void, Error>) in
            manager.removeFromPreferences { error in
                Self.resume(continuation, error: error)
            }
        }
    }

    nonisolated private static func resume(
        _ continuation: CheckedContinuation<Void, Error>,
        error: Error?
    ) {
        if let error {
            continuation.resume(throwing: error)
        } else {
            continuation.resume(returning: ())
        }
    }

    private func restore(
        _ manager: NETransparentProxyManager,
        from backup: TransparentPreferenceBackup
    ) async throws {
        if backup.existed {
            manager.localizedDescription = backup.localizedDescription
            manager.protocolConfiguration = backup.protocolConfiguration
            manager.isEnabled = backup.enabled
            try await save(manager)
        } else {
            try await remove(manager)
        }
    }

    private func restore(
        _ manager: NEDNSProxyManager,
        from backup: DNSPreferenceBackup
    ) async throws {
        if backup.configured {
            manager.localizedDescription = backup.localizedDescription
            manager.providerProtocol = backup.providerProtocol
            manager.isEnabled = backup.enabled
            try await save(manager)
        } else {
            try await remove(manager)
        }
    }
}

private struct TransparentManagerList: @unchecked Sendable {
    let values: [NETransparentProxyManager]
}

@MainActor
private struct TransparentPreferenceBackup {
    let existed: Bool
    let localizedDescription: String?
    let protocolConfiguration: NEVPNProtocol?
    let enabled: Bool
}

@MainActor
private struct DNSPreferenceBackup {
    let configured: Bool
    let localizedDescription: String?
    let providerProtocol: NEDNSProxyProviderProtocol?
    let enabled: Bool
}
