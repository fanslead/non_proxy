import Foundation
import NonProxyNativeMessaging
import SafariServices

final class SafariWebExtensionHandler: NSObject,
    NSExtensionRequestHandling
{
    func beginRequest(with context: NSExtensionContext) {
        let request = context.inputItems.first as? NSExtensionItem
        let message = request?.userInfo?[SFExtensionMessageKey]
        let requestData = Self.jsonData(message)
        let contextBox = ExtensionContextBox(context)

        dispatchSafariRequest(requestData, contextBox: contextBox)
    }

    private static func jsonData(_ value: Any?) -> Data {
        guard let value,
              JSONSerialization.isValidJSONObject(value),
              let data = try? JSONSerialization.data(withJSONObject: value)
        else {
            return Data("{}".utf8)
        }
        return data
    }

    fileprivate static func complete(
        _ context: NSExtensionContext,
        responseData: Data
    ) {
        let value = try? JSONSerialization.jsonObject(with: responseData)
        let response = NSExtensionItem()
        response.userInfo = [
            SFExtensionMessageKey: value ?? [
                "protocolVersion": 1,
                "requestID": "invalid-request",
                "ok": false,
                "error": [
                    "code": "NP_NATIVE_RUNTIME_UNAVAILABLE",
                    "message": "NonProxy 本地服务暂时不可用。",
                ],
            ],
        ]
        context.completeRequest(
            returningItems: [response],
            completionHandler: nil
        )
    }
}

private func dispatchSafariRequest(
    _ requestData: Data,
    contextBox: ExtensionContextBox
) {
    Task.detached {
        let responseData = await SafariNativeRuntime.shared.process(
            requestData
        )
        SafariWebExtensionHandler.complete(
            contextBox.value,
            responseData: responseData
        )
    }
}

private final class ExtensionContextBox: @unchecked Sendable {
    let value: NSExtensionContext

    init(_ value: NSExtensionContext) {
        self.value = value
    }
}

private actor SafariNativeRuntime {
    static let shared = SafariNativeRuntime()

    private var processor: NativeRequestProcessor?

    func process(_ requestData: Data) async -> Data {
        do {
            let processor = try activeProcessor()
            return try await processor.process(requestData)
        } catch let error as NativeMessagingError {
            return failureData(for: requestData, error: error)
        } catch {
            return failureData(
                for: requestData,
                error: .runtimeUnavailable(
                    "NonProxy 本地服务暂时不可用。"
                )
            )
        }
    }

    private func activeProcessor() throws -> NativeRequestProcessor {
        if let processor {
            return processor
        }
        let container = FileManager.default.containerURL(
            forSecurityApplicationGroupIdentifier:
                "group.com.nonproxy.shared"
        )
        let configuration = try NativeHostRuntimeConfiguration.appExtension(
            appGroupContainer: container
        )
        let service = try NativeLearningClient(
            configuration: configuration
        )
        let value = NativeRequestProcessor(
            handler: NativeRequestHandler(service: service)
        )
        processor = value
        return value
    }

    private func failureData(
        for requestData: Data,
        error: NativeMessagingError
    ) -> Data {
        if let processor {
            return processor.failure(for: requestData, error: error)
        }
        let unavailable = UnavailableLearningService()
        let fallback = NativeRequestProcessor(
            handler: NativeRequestHandler(service: unavailable)
        )
        return fallback.failure(for: requestData, error: error)
    }
}

private struct UnavailableLearningService: NativeLearningServing {
    func start(
        _ payload: StartLearningPayload
    ) async throws -> StartLearningResult {
        throw unavailable()
    }

    func observe(
        _ payload: ObserveLearningPayload
    ) async throws -> ObservationResult {
        throw unavailable()
    }

    func list(
        _ payload: SessionPayload
    ) async throws -> CandidateListResult {
        throw unavailable()
    }

    func stop(
        _ payload: SessionPayload
    ) async throws -> StopLearningResult {
        throw unavailable()
    }

    func confirm(
        _ payload: ConfirmLearningPayload
    ) async throws -> ConfirmLearningResult {
        throw unavailable()
    }

    func shutdown() {}

    private func unavailable() -> NativeMessagingError {
        .runtimeUnavailable("NonProxy 本地服务暂时不可用。")
    }
}
