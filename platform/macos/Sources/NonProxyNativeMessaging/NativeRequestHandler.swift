import Foundation

public struct NativeRequestHandler: Sendable {
    private let service: any NativeLearningServing

    public init(service: any NativeLearningServing) {
        self.service = service
    }

    public func handle(_ request: NativeRequest) async -> NativeResponse {
        do {
            let payload: NativeResponsePayload
            switch request.payload {
            case .hello:
                payload = .hello(
                    HelloResult(
                        hostVersion: "0.0.1",
                        capabilities: [
                            "site-learning-v1",
                            "domain-only-observation-v1",
                            "tab-context-isolation-v1",
                            "atomic-candidate-confirmation-v1",
                        ]
                    )
                )
            case .start(let value):
                payload = .started(try await service.start(value))
            case .observe(let value):
                payload = .observed(try await service.observe(value))
            case .list(let value):
                payload = .candidates(try await service.list(value))
            case .stop(let value):
                payload = .stopped(try await service.stop(value))
            case .confirm(let value):
                payload = .confirmed(
                    try await service.confirm(value)
                )
            }
            return .success(
                requestID: request.requestID,
                payload: payload
            )
        } catch let error as NativeMessagingError {
            return .failure(
                requestID: request.requestID,
                error: error
            )
        } catch {
            return .failure(
                requestID: request.requestID,
                error: .runtimeUnavailable(
                    "NonProxy 本地服务暂时不可用。"
                )
            )
        }
    }

    public func shutdown() {
        service.shutdown()
    }
}
