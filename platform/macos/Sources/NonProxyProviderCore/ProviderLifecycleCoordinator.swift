import Foundation
import Synchronization

public final class ProviderLifecycleCoordinator: Sendable {
    public typealias MetricsReader = @Sendable () -> ProviderHealthMetrics

    private struct State: Sendable {
        var runID: UUID?
        var status = ProviderLifecycleStatus(
            connectivity: .starting,
            activeSnapshotVersion: 0,
            lastSuccessfulRefresh: nil,
            lastErrorCode: nil
        )
        var refreshTask: Task<Void, Never>?
    }

    private let control: any ProviderControlServing
    private let session: ProviderSession
    private let cache: PolicySnapshotCache
    private let runtime: ProviderPolicyRuntime
    private let metricsReader: MetricsReader
    private let refreshInterval: Duration
    private let sessionRenewalThreshold: TimeInterval
    private let state = Mutex(State())

    public init(
        control: any ProviderControlServing,
        session: ProviderSession,
        cache: PolicySnapshotCache,
        runtime: ProviderPolicyRuntime,
        refreshInterval: Duration = .seconds(5),
        sessionRenewalThreshold: TimeInterval = 60,
        metricsReader: @escaping MetricsReader = { .idle }
    ) {
        self.control = control
        self.session = session
        self.cache = cache
        self.runtime = runtime
        self.refreshInterval = refreshInterval
        self.sessionRenewalThreshold = sessionRenewalThreshold
        self.metricsReader = metricsReader
    }

    public var status: ProviderLifecycleStatus {
        state.withLock { $0.status }
    }

    public func start() async throws {
        let runID = UUID()
        let claimedStart = state.withLock { state -> Bool in
            guard state.runID == nil else {
                return false
            }
            state.runID = runID
            return true
        }
        guard claimedStart else {
            throw ProviderError.lifecycle("Provider 生命周期已经启动")
        }

        let hasCachedSnapshot: Bool
        do {
            hasCachedSnapshot = try await installCachedSnapshot(runID: runID)
        } catch {
            releaseStart(runID: runID)
            throw error
        }
        do {
            try await synchronize(register: true, runID: runID)
        } catch {
            guard hasCachedSnapshot else {
                recordFailure(
                    error,
                    connectivity: .starting,
                    runID: runID
                )
                releaseStart(runID: runID)
                throw error
            }
            recordFailure(
                error,
                connectivity: .usingCachedSnapshot,
                runID: runID
            )
        }

        let task = Task { [weak self] in
            while !Task.isCancelled {
                guard let refreshInterval = self?.refreshInterval else {
                    return
                }
                do {
                    try await Task.sleep(for: refreshInterval)
                    try Task.checkCancellation()
                    try await self?.refresh(runID: runID)
                } catch is CancellationError {
                    return
                } catch {
                    self?.recordFailure(
                        error,
                        connectivity: .usingCachedSnapshot,
                        runID: runID
                    )
                }
            }
        }
        let accepted = state.withLock { state -> Bool in
            guard state.runID == runID else {
                return false
            }
            state.refreshTask = task
            return true
        }
        guard accepted else {
            task.cancel()
            throw CancellationError()
        }
    }

    public func stop() {
        let task = state.withLock { state -> Task<Void, Never>? in
            let current = state.refreshTask
            state.refreshTask = nil
            state.runID = nil
            state.status = ProviderLifecycleStatus(
                connectivity: .stopped,
                activeSnapshotVersion: runtime.activeSnapshotVersion,
                lastSuccessfulRefresh: state.status.lastSuccessfulRefresh,
                lastErrorCode: state.status.lastErrorCode
            )
            return current
        }
        task?.cancel()
    }

    public func refreshNow() async throws {
        guard let runID = state.withLock({ $0.runID }) else {
            throw ProviderError.lifecycle("Provider 生命周期尚未启动")
        }
        try await refresh(runID: runID)
    }

    private func refresh(runID: UUID) async throws {
        try ensureActive(runID: runID)
        let shouldRegister = !(await session.isUsable(
            minimumRemainingLifetime: sessionRenewalThreshold
        ))
        try await synchronize(register: shouldRegister, runID: runID)
    }

    private func installCachedSnapshot(runID: UUID) async throws -> Bool {
        guard let snapshot = try await cache.load() else {
            return false
        }
        try ensureActive(runID: runID)
        try runtime.install(snapshot)
        state.withLock {
            guard $0.runID == runID else {
                return
            }
            $0.status = ProviderLifecycleStatus(
                connectivity: .usingCachedSnapshot,
                activeSnapshotVersion: snapshot.version,
                lastSuccessfulRefresh: nil,
                lastErrorCode: nil
            )
        }
        return true
    }

    private func synchronize(register: Bool, runID: UUID) async throws {
        let knownVersion = runtime.activeSnapshotVersion
        let result = if register {
            try await control.synchronize(
                knownSnapshotVersion: knownVersion,
                metrics: metricsReader()
            )
        } else {
            try await control.refresh(
                knownSnapshotVersion: knownVersion,
                metrics: metricsReader()
            )
        }
        try ensureActive(runID: runID)
        if let snapshot = result.snapshot {
            try runtime.install(snapshot)
        }
        state.withLock {
            guard $0.runID == runID else {
                return
            }
            $0.status = ProviderLifecycleStatus(
                connectivity: .connected,
                activeSnapshotVersion: runtime.activeSnapshotVersion,
                lastSuccessfulRefresh: Date(),
                lastErrorCode: nil
            )
        }
    }

    private func recordFailure(
        _ error: Error,
        connectivity: ProviderConnectivity,
        runID: UUID
    ) {
        let code = (error as? ProviderError)?.code
            ?? "NP_PROVIDER_CONTROL_UNAVAILABLE"
        state.withLock {
            guard $0.runID == runID else {
                return
            }
            $0.status = ProviderLifecycleStatus(
                connectivity: connectivity,
                activeSnapshotVersion: runtime.activeSnapshotVersion,
                lastSuccessfulRefresh: $0.status.lastSuccessfulRefresh,
                lastErrorCode: code
            )
        }
    }

    private func ensureActive(runID: UUID) throws {
        guard state.withLock({ $0.runID == runID }) else {
            throw CancellationError()
        }
    }

    private func releaseStart(runID: UUID) {
        state.withLock {
            guard $0.runID == runID else {
                return
            }
            $0.runID = nil
        }
    }
}
