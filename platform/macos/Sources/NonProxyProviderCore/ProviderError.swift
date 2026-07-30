import Foundation

public enum ProviderError: Error, Equatable, LocalizedError, Sendable {
    case invalidConfiguration(String)
    case registrationRejected(String)
    case invalidSession(String)
    case invalidSnapshot(String)
    case snapshotCache(String)
    case lifecycle(String)
    case control(String)
    case dnsResolution(code: String, message: String)

    public var code: String {
        switch self {
        case .invalidConfiguration:
            "NP_PROVIDER_CONFIGURATION_INVALID"
        case .registrationRejected:
            "NP_PROVIDER_REGISTRATION_REJECTED"
        case .invalidSession:
            "NP_PROVIDER_SESSION_INVALID"
        case .invalidSnapshot:
            "NP_PROVIDER_SNAPSHOT_INVALID"
        case .snapshotCache:
            "NP_PROVIDER_SNAPSHOT_CACHE_FAILED"
        case .lifecycle:
            "NP_PROVIDER_LIFECYCLE_FAILED"
        case .control:
            "NP_PROVIDER_CONTROL_UNAVAILABLE"
        case .dnsResolution(let code, _):
            code.isEmpty ? "NP_DNS_RESOLUTION_FAILED" : code
        }
    }

    public var errorDescription: String? {
        switch self {
        case .invalidConfiguration(let message),
             .registrationRejected(let message),
             .invalidSession(let message),
             .invalidSnapshot(let message),
             .snapshotCache(let message),
             .lifecycle(let message),
             .control(let message):
            message
        case .dnsResolution(_, let message):
            message
        }
    }
}
