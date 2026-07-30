import Foundation

public enum ProviderError: Error, Equatable, LocalizedError, Sendable {
    case invalidConfiguration(String)
    case registrationRejected(String)
    case invalidSession(String)
    case invalidSnapshot(String)
    case snapshotCache(String)

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
        }
    }

    public var errorDescription: String? {
        switch self {
        case .invalidConfiguration(let message),
             .registrationRejected(let message),
             .invalidSession(let message),
             .invalidSnapshot(let message),
             .snapshotCache(let message):
            message
        }
    }
}
