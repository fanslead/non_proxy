import Foundation

public struct NativeRequestProcessor: Sendable {
    private let codec: NativeMessageCodec
    private let handler: NativeRequestHandler

    public init(
        codec: NativeMessageCodec = NativeMessageCodec(),
        handler: NativeRequestHandler
    ) {
        self.codec = codec
        self.handler = handler
    }

    public func process(_ data: Data) async throws -> Data {
        let response: NativeResponse
        do {
            response = await handler.handle(try codec.decode(data))
        } catch let error as NativeMessagingError {
            response = .failure(
                requestID: "invalid-request",
                error: error
            )
        } catch {
            response = .failure(
                requestID: "invalid-request",
                error: .invalidMessage(
                    "Native Messaging JSON 结构无效。"
                )
            )
        }
        return try codec.encode(response)
    }

    public func failure(
        for data: Data,
        error: NativeMessagingError
    ) -> Data {
        let requestID = (try? codec.decode(data).requestID)
            ?? "invalid-request"
        let response = NativeResponse.failure(
            requestID: requestID,
            error: error
        )
        return (try? codec.encode(response))
            ?? Data(Self.fallbackFailure.utf8)
    }

    public func shutdown() {
        handler.shutdown()
    }

    private static let fallbackFailure =
        #"{"error":{"code":"NP_NATIVE_RUNTIME_UNAVAILABLE","message":"NonProxy 本地服务暂时不可用。"},"ok":false,"protocolVersion":1,"requestID":"invalid-request"}"#
}
