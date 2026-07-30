@preconcurrency import SystemExtensions
import Foundation

enum SystemExtensionAction {
    case activate
    case deactivate
}

@MainActor
final class SystemExtensionMutationRequest:
    NSObject,
    @preconcurrency OSSystemExtensionRequestDelegate
{
    private var continuation:
        CheckedContinuation<SystemExtensionMutationOutcome, Error>?
    private var selfRetain: SystemExtensionMutationRequest?
    private var approvalHandler: (() -> Void)?

    func execute(
        bundleIdentifier: String,
        action: SystemExtensionAction,
        approvalHandler: @escaping () -> Void
    ) async throws -> SystemExtensionMutationOutcome {
        self.approvalHandler = approvalHandler
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
            self.selfRetain = self

            let request: OSSystemExtensionRequest
            switch action {
            case .activate:
                request = .activationRequest(
                    forExtensionWithIdentifier: bundleIdentifier,
                    queue: .main
                )
            case .deactivate:
                request = .deactivationRequest(
                    forExtensionWithIdentifier: bundleIdentifier,
                    queue: .main
                )
            }
            request.delegate = self
            OSSystemExtensionManager.shared.submitRequest(request)
        }
    }

    func request(
        _ request: OSSystemExtensionRequest,
        actionForReplacingExtension existing: OSSystemExtensionProperties,
        withExtension ext: OSSystemExtensionProperties
    ) -> OSSystemExtensionRequest.ReplacementAction {
        .replace
    }

    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) {
        approvalHandler?()
    }

    func request(
        _ request: OSSystemExtensionRequest,
        didFinishWithResult result: OSSystemExtensionRequest.Result
    ) {
        finish(.success(SystemExtensionMutationOutcome(
            requiresReboot: result == .willCompleteAfterReboot
        )))
    }

    func request(
        _ request: OSSystemExtensionRequest,
        didFailWithError error: Error
    ) {
        finish(.failure(error))
    }

    private func finish(
        _ result: Result<SystemExtensionMutationOutcome, Error>
    ) {
        guard let continuation else {
            return
        }
        self.continuation = nil
        approvalHandler = nil
        continuation.resume(with: result)
        selfRetain = nil
    }
}

@MainActor
final class SystemExtensionPropertiesRequest:
    NSObject,
    @preconcurrency OSSystemExtensionRequestDelegate
{
    private var continuation:
        CheckedContinuation<[SystemExtensionSnapshot], Error>?
    private var selfRetain: SystemExtensionPropertiesRequest?
    func execute(bundleIdentifier: String) async throws
        -> [SystemExtensionSnapshot]
    {
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
            self.selfRetain = self
            let request = OSSystemExtensionRequest.propertiesRequest(
                forExtensionWithIdentifier: bundleIdentifier,
                queue: .main
            )
            request.delegate = self
            OSSystemExtensionManager.shared.submitRequest(request)
        }
    }

    func request(
        _ request: OSSystemExtensionRequest,
        actionForReplacingExtension existing: OSSystemExtensionProperties,
        withExtension ext: OSSystemExtensionProperties
    ) -> OSSystemExtensionRequest.ReplacementAction {
        .replace
    }

    func requestNeedsUserApproval(_ request: OSSystemExtensionRequest) {}

    func request(
        _ request: OSSystemExtensionRequest,
        didFinishWithResult result: OSSystemExtensionRequest.Result
    ) {
        finish(.success([]))
    }

    func request(
        _ request: OSSystemExtensionRequest,
        didFailWithError error: Error
    ) {
        finish(.failure(error))
    }

    func request(
        _ request: OSSystemExtensionRequest,
        foundProperties properties: [OSSystemExtensionProperties]
    ) {
        let snapshots = properties.map { property in
            SystemExtensionSnapshot(
                bundleIdentifier: property.bundleIdentifier,
                installed: true,
                enabled: property.isEnabled,
                awaitingUserApproval: property.isAwaitingUserApproval,
                uninstalling: property.isUninstalling,
                bundleVersion: property.bundleVersion,
                bundleShortVersion: property.bundleShortVersion
            )
        }
        finish(.success(snapshots))
    }

    private func finish(
        _ result: Result<[SystemExtensionSnapshot], Error>
    ) {
        guard let continuation else {
            return
        }
        self.continuation = nil
        continuation.resume(with: result)
        selfRetain = nil
    }
}
