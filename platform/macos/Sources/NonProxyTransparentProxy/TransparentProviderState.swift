import Foundation
import NonProxyMacPlatformSupport
import NonProxyProviderCore
import Synchronization

struct TransparentProviderRuntime: Sendable {
    let runID: UUID
    let provider: MacProviderRuntimeComponents
    let networkEnvironment: MacNetworkEnvironmentMonitor
    let directRelays: DirectFlowRelayCoordinator
    let proxyRelays: ProxyFlowRelayCoordinator
}

final class TransparentProviderState: Sendable {
    private struct State: Sendable {
        var runID: UUID?
        var runtime: TransparentProviderRuntime?
    }

    private let state = Mutex(State())

    func beginStart() throws -> UUID {
        try state.withLock {
            guard $0.runID == nil, $0.runtime == nil else {
                throw ProviderError.lifecycle("Transparent Provider 已经启动")
            }
            let runID = UUID()
            $0.runID = runID
            return runID
        }
    }

    func install(
        _ value: TransparentProviderRuntime,
        runID: UUID
    ) -> Bool {
        state.withLock {
            guard $0.runID == runID else {
                return false
            }
            $0.runtime = value
            return true
        }
    }

    func failStart(runID: UUID) {
        state.withLock {
            guard $0.runID == runID, $0.runtime == nil else {
                return
            }
            $0.runID = nil
        }
    }

    func runtime() -> TransparentProviderRuntime? {
        state.withLock { $0.runtime }
    }

    func isCurrentStart(runID: UUID) -> Bool {
        state.withLock { $0.runID == runID }
    }

    func remove() -> TransparentProviderRuntime? {
        state.withLock {
            let current = $0.runtime
            $0.runID = nil
            $0.runtime = nil
            return current
        }
    }
}
