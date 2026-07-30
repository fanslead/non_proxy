import Foundation

public enum ProviderConnectivity: Sendable, Equatable {
    case starting
    case connected
    case usingCachedSnapshot
    case stopped
}

public struct ProviderLifecycleStatus: Sendable, Equatable {
    public let connectivity: ProviderConnectivity
    public let activeSnapshotVersion: UInt64
    public let lastSuccessfulRefresh: Date?
    public let lastErrorCode: String?

    public init(
        connectivity: ProviderConnectivity,
        activeSnapshotVersion: UInt64,
        lastSuccessfulRefresh: Date?,
        lastErrorCode: String?
    ) {
        self.connectivity = connectivity
        self.activeSnapshotVersion = activeSnapshotVersion
        self.lastSuccessfulRefresh = lastSuccessfulRefresh
        self.lastErrorCode = lastErrorCode
    }
}
