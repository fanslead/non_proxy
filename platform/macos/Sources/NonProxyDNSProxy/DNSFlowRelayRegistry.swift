import Foundation
import Synchronization

protocol DNSFlowRelay: AnyObject, Sendable {
    var id: UUID { get }
    func start()
    func cancel()
}

final class DNSFlowRelayRegistry: Sendable {
    private struct State: Sendable {
        var accepting = false
        var relays: [UUID: any DNSFlowRelay] = [:]
    }

    private let capacity: Int
    private let state = Mutex(State())

    init(capacity: Int = 1_024) {
        self.capacity = max(1, capacity)
    }

    var activeFlowCount: UInt64 {
        state.withLock { UInt64($0.relays.count) }
    }

    func beginAccepting() {
        state.withLock { $0.accepting = true }
    }

    func insert(_ relay: any DNSFlowRelay) -> Bool {
        state.withLock {
            guard $0.accepting, $0.relays.count < capacity else {
                return false
            }
            $0.relays[relay.id] = relay
            return true
        }
    }

    func remove(id: UUID) {
        _ = state.withLock { $0.relays.removeValue(forKey: id) }
    }

    func stopAcceptingAndCancelAll() {
        let relays = state.withLock { state -> [any DNSFlowRelay] in
            state.accepting = false
            let current = Array(state.relays.values)
            state.relays.removeAll(keepingCapacity: false)
            return current
        }
        relays.forEach { $0.cancel() }
    }
}
