public protocol ProviderControlServing: Sendable {
    func synchronize(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics
    ) async throws -> ProviderSynchronizationResult

    func refresh(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics
    ) async throws -> ProviderSynchronizationResult
}
