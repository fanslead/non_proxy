import Foundation

public enum DNSProxyError: Error, Equatable, LocalizedError, Sendable {
    case invalidMessage(String)
    case unsupportedQuery(String)
    case resolverUnavailable(String)
    case providerUnavailable(String)
    case capacityExceeded(String)
    case responseInvalid(String)
    case flow(String)

    public var errorDescription: String? {
        switch self {
        case .invalidMessage(let message),
             .unsupportedQuery(let message),
             .resolverUnavailable(let message),
             .providerUnavailable(let message),
             .capacityExceeded(let message),
             .responseInvalid(let message),
             .flow(let message):
            message
        }
    }
}
