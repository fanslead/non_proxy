import Synchronization

protocol FlowRelay: AnyObject, Sendable {
    func start()
    func cancel()
}

final class FlowRelayRegistry: Sendable {
    private static let maximumActiveFlows = 2_048
    private static let maximumQueuedBytes = 32 * 1024 * 1024

    private struct State {
        var relays: [ObjectIdentifier: any FlowRelay] = [:]
        var queuedBytes = 0
        var isAccepting = false
    }

    private let state = Mutex(State())

    var activeFlowCount: UInt64 {
        state.withLock { UInt64($0.relays.count) }
    }

    var queuedBytes: UInt64 {
        state.withLock { UInt64($0.queuedBytes) }
    }

    func beginAccepting() {
        state.withLock {
            $0.isAccepting = true
        }
    }

    func insert(_ relay: any FlowRelay) -> Bool {
        state.withLock {
            guard $0.isAccepting,
                  $0.relays.count < Self.maximumActiveFlows
            else {
                return false
            }
            $0.relays[ObjectIdentifier(relay)] = relay
            return true
        }
    }

    func remove(_ relay: any FlowRelay) {
        _ = state.withLock {
            $0.relays.removeValue(forKey: ObjectIdentifier(relay))
        }
    }

    func reserve(bytes: Int) -> Bool {
        guard bytes >= 0, bytes <= Self.maximumQueuedBytes else {
            return false
        }
        return state.withLock {
            guard $0.isAccepting,
                  $0.queuedBytes <= Self.maximumQueuedBytes - bytes
            else {
                return false
            }
            $0.queuedBytes += bytes
            return true
        }
    }

    func release(bytes: Int) {
        guard bytes >= 0 else {
            return
        }
        state.withLock {
            $0.queuedBytes = max(0, $0.queuedBytes - bytes)
        }
    }

    func stopAcceptingAndCancelAll() {
        let retained = state.withLock { state -> [any FlowRelay] in
            state.isAccepting = false
            let current = Array(state.relays.values)
            state.relays.removeAll(keepingCapacity: false)
            return current
        }
        retained.forEach { $0.cancel() }
    }
}
