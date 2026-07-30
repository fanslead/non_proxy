import Network
import Synchronization

// NWPathMonitor 与 continuation 状态由 Mutex 保护。
final class PhysicalInterfaceCatalog: @unchecked Sendable {
    private static let initialPathTimeout: DispatchTimeInterval = .seconds(3)

    private struct State {
        var interfaces: [NWInterface] = []
        var continuation: CheckedContinuation<Void, Never>?
        var started = false
    }

    private let monitor = NWPathMonitor()
    private let queue = DispatchQueue(
        label: "com.nonproxy.transparent.physical-path"
    )
    private let state = Mutex(State())

    func start() async {
        await withCheckedContinuation { continuation in
            let shouldStart = state.withLock { state -> Bool in
                guard !state.started else {
                    continuation.resume()
                    return false
                }
                state.started = true
                state.continuation = continuation
                return true
            }
            guard shouldStart else {
                return
            }
            monitor.pathUpdateHandler = { [weak self] path in
                self?.update(path)
            }
            monitor.start(queue: queue)
            queue.asyncAfter(
                deadline: .now() + Self.initialPathTimeout
            ) { [weak self] in
                self?.finishInitialWait()
            }
        }
    }

    func stop() {
        monitor.cancel()
        let continuation = state.withLock { state -> CheckedContinuation<Void, Never>? in
            let current = state.continuation
            state.continuation = nil
            state.interfaces = []
            return current
        }
        continuation?.resume()
    }

    func preferredInterface() -> NWInterface? {
        state.withLock { $0.interfaces.first }
    }

    static func priority(for type: NWInterface.InterfaceType) -> Int? {
        switch type {
        case .wiredEthernet:
            0
        case .wifi:
            1
        case .cellular:
            2
        default:
            nil
        }
    }

    private func update(_ path: NWPath) {
        let physical = path.availableInterfaces
            .compactMap { interface -> (NWInterface, Int)? in
                guard let priority = Self.priority(for: interface.type) else {
                    return nil
                }
                return (interface, priority)
            }
            .sorted {
                if $0.1 != $1.1 {
                    return $0.1 < $1.1
                }
                return $0.0.name < $1.0.name
            }
            .map(\.0)
        let continuation = state.withLock { state -> CheckedContinuation<Void, Never>? in
            state.interfaces = physical
            let current = state.continuation
            state.continuation = nil
            return current
        }
        continuation?.resume()
    }

    private func finishInitialWait() {
        let continuation = state.withLock { state -> CheckedContinuation<Void, Never>? in
            let current = state.continuation
            state.continuation = nil
            return current
        }
        continuation?.resume()
    }
}
