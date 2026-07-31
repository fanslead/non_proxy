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
        let systemExtensionApprovalHandler = {
            sink.progress(BridgeEventPayload(
                operation: .installAndEnable,
                success: true,
                message: "请在“系统设置 → 通用 → 登录项与扩展”中允许 NonProxy。",
                errorCode: nil,
                requiresReboot: false,
                state: nil
            ))
        }
        let gatewayController = GatewayAgentController()
        let adapterHostController = AdapterHostAgentController()
        let backgroundApprovalHandler = {
            sink.progress(BridgeEventPayload(
                operation: .installAndEnable,
                success: true,
                message:
                    "请在“系统设置 → 通用 → 登录项与扩展”中允许 NonProxy 后台项目。",
                errorCode: nil,
                requiresReboot: false,
                state: nil
            ))
        }
        let manifestController = NativeMessagingManifestController()
        let prepareForBackgroundReplacement: () async throws -> Void = {
            sink.progress(BridgeEventPayload(
                operation: .installAndEnable,
                success: true,
                message:
                    "检测到后台服务需要升级或重启，正在先停用网络接管。",
                errorCode: nil,
                requiresReboot: false,
                state: nil
            ))
            try await NetworkPreferencesController().disableAndRemove()
        }

        do {
            let gatewayOutcome = try await gatewayController.registerAndWait(
                approvalHandler: backgroundApprovalHandler,
                prepareForReplacement: prepareForBackgroundReplacement
            )
            let adapterHostOutcome: BackgroundAgentRegistrationOutcome
            do {
                adapterHostOutcome = try await adapterHostController
                    .registerAndWait(
                        approvalHandler: backgroundApprovalHandler,
                        prepareForReplacement:
                            prepareForBackgroundReplacement
                    )
            } catch {
                let rollbackErrors = await rollbackBackgroundAgents(
                    adapterHostController: adapterHostController,
                    adapterHostOutcome: nil,
                    gatewayController: gatewayController,
                    gatewayOutcome: gatewayOutcome
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
            let manifestBackups: [
                NativeMessagingManifestController.Backup
            ]
            do {
                manifestBackups = try manifestController.install()
            } catch {
                let rollbackErrors = await rollbackBackgroundAgents(
                    adapterHostController: adapterHostController,
                    adapterHostOutcome: adapterHostOutcome,
                    gatewayController: gatewayController,
                    gatewayOutcome: gatewayOutcome
                )
                if !rollbackErrors.isEmpty {
                    throw BridgeError(
                        code: "NP_MAC_INSTALL_ROLLBACK_FAILED",
                        message:
                            "\(error.localizedDescription)；"
                            + rollbackErrors.joined(separator: "；")
                    )
                }
                throw error
            }
            do {
                try await installNetworkComponents(
                    systemController: systemController,
                    systemExtensionApprovalHandler:
                        systemExtensionApprovalHandler,
                    sink: sink
                )
            } catch {
                var rollbackErrors: [String] = []
                do {
                    try manifestController.restore(manifestBackups)
                } catch let manifestError {
                    rollbackErrors.append(
                        "浏览器宿主清单回滚失败："
                            + manifestError.localizedDescription
                    )
                }
                rollbackErrors.append(contentsOf:
                    await rollbackBackgroundAgents(
                        adapterHostController: adapterHostController,
                        adapterHostOutcome: adapterHostOutcome,
                        gatewayController: gatewayController,
                        gatewayOutcome: gatewayOutcome
                    ))
                if !rollbackErrors.isEmpty {
                    throw BridgeError(
                        code: "NP_MAC_INSTALL_ROLLBACK_FAILED",
                        message:
                            "\(error.localizedDescription)；"
                            + rollbackErrors.joined(separator: "；")
                    )
                }
                throw error
            }
        } catch {
            sink.complete(.failure(
                operation: .installAndEnable,
                error: error
            ))
        }
    }

    private static func installNetworkComponents(
        systemController: SystemExtensionController,
        systemExtensionApprovalHandler: @escaping () -> Void,
        sink: BridgeCallbackSink
    ) async throws {
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
            approvalHandler: systemExtensionApprovalHandler
        )
        let dnsOutcome: SystemExtensionMutationOutcome
        do {
            dnsOutcome = try await systemController.activate(
                bundleIdentifier: BridgeConstants.dnsBundleIdentifier,
                approvalHandler: systemExtensionApprovalHandler
            )
        } catch {
            if !previousTransparent.enabled {
                do {
                    _ = try await systemController.deactivate(
                        bundleIdentifier:
                            BridgeConstants.transparentBundleIdentifier
                    )
                } catch let rollbackError {
                    throw BridgeError(
                        code: "NP_MAC_INSTALL_ROLLBACK_FAILED",
                        message:
                            "\(error.localizedDescription)；透明代理扩展回滚失败："
                            + rollbackError.localizedDescription
                    )
                }
            }
            throw error
        }

        let requiresReboot =
            transparentOutcome.requiresReboot
            || dnsOutcome.requiresReboot
        if requiresReboot {
            sink.complete(.success(
                operation: .installAndEnable,
                message: "后台服务和系统扩展已登记，重启电脑后才能启用网络配置。",
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
            message: "后台服务、系统扩展和网络配置均已启用。",
            state: try? await queryState()
        ))
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
            do {
                try await AdapterHostAgentController().unregister()
            } catch {
                errors.append(error.localizedDescription)
            }
            do {
                try await GatewayAgentController().unregister()
            } catch {
                errors.append(error.localizedDescription)
            }
            do {
                try NativeMessagingManifestController().uninstall()
            } catch {
                errors.append(error.localizedDescription)
            }
            guard errors.isEmpty else {
                throw BridgeError(
                    code: "NP_MAC_COMPONENT_REMOVE_FAILED",
                    message:
                        "网络配置已停用，但部分系统组件未卸载："
                        + errors.joined(separator: "；")
                )
            }

            let message = requiresReboot
                ? "网络配置和后台服务已停用，重启电脑后完成系统扩展卸载。"
                : "网络配置、系统扩展和后台服务均已卸载。"
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
        let gatewayAgent = GatewayAgentController().query()
        let adapterHostAgent = AdapterHostAgentController().query()
        let systemController = SystemExtensionController()
        let transparent = try await systemController.query(
            bundleIdentifier: BridgeConstants.transparentBundleIdentifier
        )
        let dns = try await systemController.query(
            bundleIdentifier: BridgeConstants.dnsBundleIdentifier
        )
        let preferences = try await NetworkPreferencesController().query()
        return MacHostState(
            gatewayAgent: gatewayAgent,
            adapterHostAgent: adapterHostAgent,
            transparentExtension: transparent,
            dnsExtension: dns,
            transparentPreference: preferences.transparent,
            dnsPreference: preferences.dns
        )
    }

    static func stateMessage(_ state: MacHostState) -> String {
        if !state.gatewayAgent.found {
            return "当前安装包缺少 gatewayd 后台项目。"
        }
        if !state.adapterHostAgent.found {
            return "当前安装包缺少 adapter-host 后台项目。"
        }
        if state.gatewayAgent.requiresApproval
            || state.adapterHostAgent.requiresApproval
            || state.transparentExtension.awaitingUserApproval
            || state.dnsExtension.awaitingUserApproval
        {
            return "系统正在等待用户允许 NonProxy 后台项目或网络扩展。"
        }
        if state.gatewayAgent.requiresUpgrade {
            return "gatewayd 版本与当前安装包不一致，需要安全升级。"
        }
        if state.adapterHostAgent.requiresUpgrade {
            return "adapter-host 版本与当前安装包不一致，需要安全升级。"
        }
        if state.gatewayAgent.enabled,
           state.gatewayAgent.ready,
           state.adapterHostAgent.enabled,
           state.adapterHostAgent.ready,
           state.transparentExtension.enabled,
           state.dnsExtension.enabled,
           state.transparentPreference.enabled,
           state.dnsPreference.enabled
        {
            return "后台服务、系统扩展和网络配置均已就绪。"
        }
        if !state.gatewayAgent.registered,
           !state.adapterHostAgent.registered,
           !state.transparentExtension.installed,
           !state.dnsExtension.installed,
           !state.transparentPreference.configured,
           !state.dnsPreference.configured
        {
            return "NonProxy 系统组件尚未安装。"
        }
        return "NonProxy 系统组件仅部分就绪，需要修复。"
    }

    private static func rollbackBackgroundAgents(
        adapterHostController: AdapterHostAgentController,
        adapterHostOutcome: BackgroundAgentRegistrationOutcome?,
        gatewayController: GatewayAgentController,
        gatewayOutcome: BackgroundAgentRegistrationOutcome?
    ) async -> [String] {
        var errors: [String] = []
        if adapterHostOutcome?.newlyRegistered == true {
            do {
                try await adapterHostController.unregister()
            } catch {
                errors.append(
                    "adapter-host 回滚失败：" + error.localizedDescription
                )
            }
        }
        if gatewayOutcome?.newlyRegistered == true {
            do {
                try await gatewayController.unregister()
            } catch {
                errors.append(
                    "gatewayd 回滚失败：" + error.localizedDescription
                )
            }
        }
        return errors
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
