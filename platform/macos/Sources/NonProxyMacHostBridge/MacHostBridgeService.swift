import Foundation

@MainActor
enum MacHostBridgeService {
    static func query(sink: BridgeCallbackSink) async {
        do {
            let state = try await queryState()
            sink.complete(.success(
                operation: .query,
                message: stateMessage(state),
                state: state
            ))
        } catch {
            sink.complete(.failure(operation: .query, error: error))
        }
    }

    static func installAndEnable(sink: BridgeCallbackSink) async {
        let systemController = SystemExtensionController()
        let approvalHandler = {
            sink.progress(BridgeEventPayload(
                operation: .installAndEnable,
                success: true,
                message: "请在“系统设置 → 通用 → 登录项与扩展”中允许 NonProxy。",
                errorCode: nil,
                requiresReboot: false,
                state: nil
            ))
        }

        do {
            let previousTransparent = try await systemController.query(
                bundleIdentifier:
                    BridgeConstants.transparentBundleIdentifier
            )
            let previousDNS = try await systemController.query(
                bundleIdentifier: BridgeConstants.dnsBundleIdentifier
            )

            let transparentOutcome = try await systemController.activate(
                bundleIdentifier:
                    BridgeConstants.transparentBundleIdentifier,
                approvalHandler: approvalHandler
            )
            let dnsOutcome: SystemExtensionMutationOutcome
            do {
                dnsOutcome = try await systemController.activate(
                    bundleIdentifier: BridgeConstants.dnsBundleIdentifier,
                    approvalHandler: approvalHandler
                )
            } catch {
                if !previousTransparent.enabled {
                    _ = try? await systemController.deactivate(
                        bundleIdentifier:
                            BridgeConstants.transparentBundleIdentifier
                    )
                }
                throw error
            }

            let requiresReboot =
                transparentOutcome.requiresReboot
                || dnsOutcome.requiresReboot
            if requiresReboot {
                sink.complete(.success(
                    operation: .installAndEnable,
                    message: "系统扩展已登记，重启电脑后才能启用网络配置。",
                    requiresReboot: true,
                    state: try? await queryState()
                ))
                return
            }

            do {
                try await NetworkPreferencesController().enable()
            } catch {
                let rollbackErrors = await restoreExtensionActivation(
                    previousTransparent: previousTransparent,
                    previousDNS: previousDNS
                )
                guard rollbackErrors.isEmpty else {
                    throw BridgeError(
                        code: "NP_MAC_INSTALL_ROLLBACK_FAILED",
                        message:
                            "\(error.localizedDescription)；"
                            + rollbackErrors.joined(separator: "；")
                    )
                }
                throw error
            }

            sink.complete(.success(
                operation: .installAndEnable,
                message: "系统扩展和网络配置均已启用。",
                state: try? await queryState()
            ))
        } catch {
            sink.complete(.failure(
                operation: .installAndEnable,
                error: error
            ))
        }
    }

    static func disableAndUninstall(sink: BridgeCallbackSink) async {
        do {
            try await NetworkPreferencesController().disableAndRemove()

            let controller = SystemExtensionController()
            var requiresReboot = false
            var errors: [String] = []
            for bundleIdentifier in [
                BridgeConstants.dnsBundleIdentifier,
                BridgeConstants.transparentBundleIdentifier,
            ] {
                do {
                    let outcome = try await controller.deactivate(
                        bundleIdentifier: bundleIdentifier
                    )
                    requiresReboot =
                        requiresReboot || outcome.requiresReboot
                } catch {
                    errors.append(error.localizedDescription)
                }
            }
            guard errors.isEmpty else {
                throw BridgeError(
                    code: "NP_MAC_SYSTEM_EXTENSION_REMOVE_FAILED",
                    message:
                        "网络配置已停用，但部分系统扩展未卸载："
                        + errors.joined(separator: "；")
                )
            }

            let message = requiresReboot
                ? "网络配置已停用，重启电脑后完成系统扩展卸载。"
                : "网络配置和系统扩展均已卸载。"
            sink.complete(.success(
                operation: .disableAndUninstall,
                message: message,
                requiresReboot: requiresReboot,
                state: try? await queryState()
            ))
        } catch {
            sink.complete(.failure(
                operation: .disableAndUninstall,
                error: error
            ))
        }
    }

    private static func queryState() async throws -> MacHostState {
        let systemController = SystemExtensionController()
        let transparent = try await systemController.query(
            bundleIdentifier: BridgeConstants.transparentBundleIdentifier
        )
        let dns = try await systemController.query(
            bundleIdentifier: BridgeConstants.dnsBundleIdentifier
        )
        let preferences = try await NetworkPreferencesController().query()
        return MacHostState(
            transparentExtension: transparent,
            dnsExtension: dns,
            transparentPreference: preferences.transparent,
            dnsPreference: preferences.dns
        )
    }

    private static func stateMessage(_ state: MacHostState) -> String {
        if state.transparentExtension.awaitingUserApproval
            || state.dnsExtension.awaitingUserApproval
        {
            return "系统正在等待用户允许 NonProxy 网络扩展。"
        }
        if state.transparentExtension.enabled,
           state.dnsExtension.enabled,
           state.transparentPreference.enabled,
           state.dnsPreference.enabled
        {
            return "系统扩展和网络配置均已就绪。"
        }
        if !state.transparentExtension.installed,
           !state.dnsExtension.installed
        {
            return "NonProxy 系统扩展尚未安装。"
        }
        return "NonProxy 系统组件仅部分就绪，需要修复。"
    }

    private static func restoreExtensionActivation(
        previousTransparent: SystemExtensionSnapshot,
        previousDNS: SystemExtensionSnapshot
    ) async -> [String] {
        let controller = SystemExtensionController()
        var errors: [String] = []
        if !previousDNS.enabled {
            do {
                _ = try await controller.deactivate(
                    bundleIdentifier: BridgeConstants.dnsBundleIdentifier
                )
            } catch {
                errors.append("DNS 扩展回滚失败：\(error.localizedDescription)")
            }
        }
        if !previousTransparent.enabled {
            do {
                _ = try await controller.deactivate(
                    bundleIdentifier:
                        BridgeConstants.transparentBundleIdentifier
                )
            } catch {
                errors.append(
                    "透明代理扩展回滚失败：\(error.localizedDescription)"
                )
            }
        }
        return errors
    }
}
