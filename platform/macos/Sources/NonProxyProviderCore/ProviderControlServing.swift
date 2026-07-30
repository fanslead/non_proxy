import NonProxyProviderContracts

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

public protocol ProviderDNSResolving: Sendable {
    func resolveDNS(
        _ request: Nonproxy_Provider_V1_ResolveDnsRequest
    ) async throws -> Nonproxy_Provider_V1_ResolveDnsResponse
}
