import Foundation
import NonProxyMacNetworkIdentity
import NonProxyMacPlatformSupport
import NonProxyProviderCore
import Synchronization

struct DNSProviderRuntime: Sendable {
    let provider: MacProviderRuntimeComponents
    let catalogs: DNSResolverCatalogStore
    let networkEnvironment: MacNetworkEnvironmentMonitor
    let coordinator: DNSQueryCoordinator
}

final class DNSProviderState: Sendable {
    private struct State: Sendable {
        var runID: UUID?
        var runtime: DNSProviderRuntime?
    }

    private let state = Mutex(State())

    func beginStart() throws -> UUID {
        try state.withLock {
            guard $0.runID == nil, $0.runtime == nil else {
                throw ProviderError.lifecycle("DNS Provider 已经启动")
            }
            let runID = UUID()
            $0.runID = runID
            return runID
        }
    }

    func install(_ runtime: DNSProviderRuntime, runID: UUID) -> Bool {
        state.withLock {
            guard $0.runID == runID else {
                return false
            }
            $0.runtime = runtime
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

    func runtime() -> DNSProviderRuntime? {
        state.withLock { $0.runtime }
    }

    func remove() -> DNSProviderRuntime? {
        state.withLock {
            let runtime = $0.runtime
            $0.runID = nil
            $0.runtime = nil
            return runtime
        }
    }
}
