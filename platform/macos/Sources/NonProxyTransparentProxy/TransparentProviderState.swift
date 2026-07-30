import Foundation
import NonProxyMacPlatformSupport
import NonProxyProviderCore
import Synchronization

final class TransparentProviderState: Sendable {
    private struct State: Sendable {
        var runID: UUID?
        var components: MacProviderRuntimeComponents?
    }

    private let state = Mutex(State())

    func beginStart() throws -> UUID {
        try state.withLock {
            guard $0.runID == nil, $0.components == nil else {
                throw ProviderError.lifecycle("Transparent Provider 已经启动")
            }
            let runID = UUID()
            $0.runID = runID
            return runID
        }
    }

    func install(
        _ value: MacProviderRuntimeComponents,
        runID: UUID
    ) -> Bool {
        state.withLock {
            guard $0.runID == runID else {
                return false
            }
            $0.components = value
            return true
        }
    }

    func failStart(runID: UUID) {
        state.withLock {
            guard $0.runID == runID, $0.components == nil else {
                return
            }
            $0.runID = nil
        }
    }

    func runtimeComponents() -> MacProviderRuntimeComponents? {
        state.withLock { $0.components }
    }

    func isCurrentStart(runID: UUID) -> Bool {
        state.withLock { $0.runID == runID }
    }

    func remove() -> MacProviderRuntimeComponents? {
        state.withLock {
            let current = $0.components
            $0.runID = nil
            $0.components = nil
            return current
        }
    }
}
