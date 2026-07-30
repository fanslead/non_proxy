import Foundation

public enum NativeMessagingError: Error, Equatable, LocalizedError, Sendable {
    case invalidCaller
    case invalidFrame
    case messageTooLarge
    case invalidMessage(String)
    case runtimeUnavailable(String)
    case gatewayRejected(code: String, message: String)

    public var code: String {
        switch self {
        case .invalidCaller:
            "NP_NATIVE_CALLER_INVALID"
        case .invalidFrame:
            "NP_NATIVE_FRAME_INVALID"
        case .messageTooLarge:
            "NP_NATIVE_MESSAGE_TOO_LARGE"
        case .invalidMessage:
            "NP_NATIVE_MESSAGE_INVALID"
        case .runtimeUnavailable:
            "NP_NATIVE_RUNTIME_UNAVAILABLE"
        case .gatewayRejected(let code, _):
            code
        }
    }

    public var errorDescription: String? {
        switch self {
        case .invalidCaller:
            "浏览器扩展来源未获授权。"
        case .invalidFrame:
            "Native Messaging 消息帧无效。"
        case .messageTooLarge:
            "Native Messaging 消息超过大小上限。"
        case .invalidMessage(let message):
            message
        case .runtimeUnavailable(let message):
            message
        case .gatewayRejected(_, let message):
            message
        }
    }
}
