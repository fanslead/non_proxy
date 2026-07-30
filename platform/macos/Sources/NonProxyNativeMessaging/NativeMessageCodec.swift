import Foundation

public struct NativeMessageCodec: Sendable {
    private struct Envelope<Payload: Decodable>: Decodable {
        let protocolVersion: Int
        let requestID: String
        let type: String
        let payload: Payload
    }

    private struct EmptyPayload: Decodable {}

    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()

    public init() {
        encoder.outputFormatting = [.sortedKeys]
    }

    public func decode(_ data: Data) throws -> NativeRequest {
        let metadata: Envelope<EmptyPayload>
        do {
            metadata = try decoder.decode(
                Envelope<EmptyPayload>.self,
                from: data
            )
        } catch {
            throw NativeMessagingError.invalidMessage(
                "Native Messaging JSON 结构无效。"
            )
        }
        guard metadata.protocolVersion == NativeRequest.protocolVersion else {
            throw NativeMessagingError.invalidMessage(
                "Native Messaging 协议版本不受支持。"
            )
        }
        try validateIdentifier(metadata.requestID, field: "requestID")

        let payload: NativeRequestPayload
        switch metadata.type {
        case "hello":
            payload = .hello
        case "startLearning":
            payload = .start(try decodePayload(data))
        case "observeLearning":
            payload = .observe(try decodePayload(data))
        case "listCandidates":
            payload = .list(try decodePayload(data))
        case "stopLearning":
            payload = .stop(try decodePayload(data))
        case "confirmLearning":
            payload = .confirm(try decodePayload(data))
        default:
            throw NativeMessagingError.invalidMessage(
                "Native Messaging 消息类型不受支持。"
            )
        }
        return NativeRequest(
            requestID: metadata.requestID,
            payload: payload
        )
    }

    public func encode(_ response: NativeResponse) throws -> Data {
        try encoder.encode(response)
    }

    private func decodePayload<T: Decodable>(_ data: Data) throws -> T {
        do {
            return try decoder.decode(Envelope<T>.self, from: data).payload
        } catch {
            throw NativeMessagingError.invalidMessage(
                "Native Messaging payload 无效。"
            )
        }
    }

    private func validateIdentifier(
        _ value: String,
        field: String
    ) throws {
        let allowed = CharacterSet(
            charactersIn:
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._:-"
        )
        guard !value.isEmpty,
              value.utf8.count <= 128,
              value.unicodeScalars.allSatisfy(allowed.contains)
        else {
            throw NativeMessagingError.invalidMessage(
                "\(field) 无效。"
            )
        }
    }
}
