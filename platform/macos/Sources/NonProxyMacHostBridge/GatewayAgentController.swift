import NonProxyMacRuntime
import ServiceManagement

@MainActor
struct GatewayAgentController {
    private let controller: BackgroundAgentController

    init(
        service: any BackgroundAgentServicing = SMAppService.agent(
            plistName: MacSharedRuntimePaths.gatewayAgentPlistName
        ),
        appGroupValidator: @escaping () throws -> Void = {
            _ = try MacSharedRuntimePaths.live()
        },
        fingerprintProvider: @escaping () throws -> String = {
            try GatewayBundleFingerprint.live()
        },
        runtimeInspector: @escaping (String) -> BackgroundRuntimeState = {
            Self.inspectLiveRuntime(expectedFingerprint: $0)
        }
    ) {
        controller = BackgroundAgentController(
            descriptor: .gateway,
            service: service,
            appGroupValidator: appGroupValidator,
            fingerprintProvider: fingerprintProvider,
            runtimeInspector: runtimeInspector
        )
    }

    func query() -> BackgroundAgentSnapshot {
        controller.query()
    }

    func registerAndWait(
        approvalHandler: @escaping () -> Void,
        prepareForReplacement: @escaping () async throws -> Void
    ) async throws -> BackgroundAgentRegistrationOutcome {
        try await controller.registerAndWait(
            approvalHandler: approvalHandler,
            prepareForReplacement: prepareForReplacement
        )
    }

    func unregister() async throws {
        try await controller.unregister()
    }

    static func snapshot(
        status: SMAppService.Status,
        runtimeReady: Bool,
        requiresUpgrade: Bool = false
    ) -> BackgroundAgentSnapshot {
        BackgroundAgentController.snapshot(
            status: status,
            runtimeReady: runtimeReady,
            requiresUpgrade: requiresUpgrade
        )
    }

    private static func inspectLiveRuntime(
        expectedFingerprint: String
    ) -> BackgroundRuntimeState {
        guard let paths = try? MacSharedRuntimePaths.live() else {
            return .notReady
        }
        do {
            try GatewayRuntimeReadiness.inspect(
                paths: paths,
                expectedFingerprint: expectedFingerprint
            )
            return .ready
        } catch GatewayRuntimeReadinessError.fingerprintMismatch,
                GatewayRuntimeReadinessError.invalidRuntimeIdentity
        {
            return .requiresReplacement
        } catch {
            return .notReady
        }
    }
}
