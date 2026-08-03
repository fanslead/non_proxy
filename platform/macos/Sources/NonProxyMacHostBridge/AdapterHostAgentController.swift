import NonProxyMacRuntime
import ServiceManagement

@MainActor
struct AdapterHostAgentController {
  private let controller: BackgroundAgentController

  init(
    service: any BackgroundAgentServicing = SMAppService.agent(
      plistName: MacSharedRuntimePaths.adapterHostAgentPlistName
    ),
    installationValidator: @escaping () throws -> Void = {
      try MacSystemInstallationEligibility.validateLive()
      _ = try MacSharedRuntimePaths.live()
    },
    fingerprintProvider: @escaping () throws -> String = {
      try AdapterHostBundleFingerprint.live()
    },
    runtimeInspector: @escaping (String) -> BackgroundRuntimeState = {
      Self.inspectLiveRuntime(expectedFingerprint: $0)
    }
  ) {
    controller = BackgroundAgentController(
      descriptor: .adapterHost,
      service: service,
      installationValidator: installationValidator,
      fingerprintProvider: fingerprintProvider,
      runtimeInspector: runtimeInspector
    )
  }

  func query() throws -> BackgroundAgentSnapshot {
    try controller.query()
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

  private static func inspectLiveRuntime(
    expectedFingerprint: String
  ) -> BackgroundRuntimeState {
    guard let paths = try? MacSharedRuntimePaths.live() else {
      return .notReady
    }
    do {
      try AdapterHostRuntimeReadiness.inspect(
        paths: paths,
        expectedFingerprint: expectedFingerprint
      )
      return .ready
    } catch AdapterHostRuntimeReadinessError.fingerprintMismatch,
      AdapterHostRuntimeReadinessError.invalidRuntimeIdentity
    {
      return .requiresReplacement
    } catch {
      return .notReady
    }
  }
}
