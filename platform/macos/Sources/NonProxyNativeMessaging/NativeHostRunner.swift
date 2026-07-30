import Foundation

public struct NativeHostRunner: Sendable {
    private let framer: NativeMessageFramer
    private let codec: NativeMessageCodec
    private let handler: NativeRequestHandler

    public init(
        framer: NativeMessageFramer = NativeMessageFramer(),
        codec: NativeMessageCodec = NativeMessageCodec(),
        handler: NativeRequestHandler
    ) {
        self.framer = framer
        self.codec = codec
        self.handler = handler
    }

    public func run(
        input: FileHandle = .standardInput,
        output: FileHandle = .standardOutput
    ) async throws {
        defer {
            handler.shutdown()
        }
        while let data = try framer.readMessage(from: input) {
            let response: NativeResponse
            do {
                let request = try codec.decode(data)
                response = await handler.handle(request)
            } catch let error as NativeMessagingError {
                response = .failure(
                    requestID: "invalid-request",
                    error: error
                )
            }
            try framer.writeMessage(
                try codec.encode(response),
                to: output
            )
        }
    }
}
