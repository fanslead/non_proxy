import CryptoKit
import Darwin
import Foundation
import Network
import Synchronization

public final class DNSNetworkProfileMonitor: Sendable {
    private static let initialPathTimeout: DispatchTimeInterval = .seconds(3)

    private struct State: Sendable {
        var signature = "unknown"
        var generation: UInt64 = 1
        var stopped = false
        var started = false
        var physicalInterfaceIndex: UInt32
        var continuation: CheckedContinuation<Void, Never>?

        init(physicalInterfaceIndex: UInt32) {
            self.physicalInterfaceIndex = physicalInterfaceIndex
        }
    }

    private let runID = UUID()
    private let monitor: NWPathMonitor
    private let queue = DispatchQueue(
        label: "com.nonproxy.dns-network-profile"
    )
    private let state: Mutex<State>

    public init(
        monitor: NWPathMonitor = NWPathMonitor(),
        initialInterfaceIndex: UInt32 = 0
    ) {
        self.monitor = monitor
        self.state = Mutex(
            State(physicalInterfaceIndex: initialInterfaceIndex)
        )
    }

    public func start() async {
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
                self?.record(path)
            }
            monitor.start(queue: queue)
            queue.asyncAfter(
                deadline: .now() + Self.initialPathTimeout
            ) { [weak self] in
                self?.finishInitialWait()
            }
        }
    }

    public func stop() {
        let result = state.withLock { state -> (
            shouldStop: Bool,
            continuation: CheckedContinuation<Void, Never>?
        ) in
            guard !state.stopped else {
                return (false, nil)
            }
            state.stopped = true
            let continuation = state.continuation
            state.continuation = nil
            return (true, continuation)
        }
        if result.shouldStop {
            monitor.cancel()
        }
        result.continuation?.resume()
    }

    public var preferredInterfaceIndex: UInt32 {
        state.withLock { $0.physicalInterfaceIndex }
    }

    public func profileID(
        upstreams: [DNSUpstreamEndpoint] = []
    ) -> String {
        let stateValue = state.withLock {
            (
                signature: $0.signature,
                generation: $0.generation,
                interfaceIndex: $0.physicalInterfaceIndex
            )
        }
        let resolvers = upstreams
            .map { "\($0.ipAddress):\($0.port):\($0.scopeID)" }
            .sorted()
            .joined(separator: ",")
        let source = [
            runID.uuidString.lowercased(),
            String(stateValue.generation),
            stateValue.signature,
            String(stateValue.interfaceIndex),
            resolvers,
        ].joined(separator: "|")
        let digest = SHA256.hash(data: Data(source.utf8))
            .prefix(8)
            .map { String(format: "%02x", $0) }
            .joined()
        return "dns-\(runID.uuidString.prefix(8).lowercased())"
            + "-\(stateValue.generation)-\(digest)"
    }

    private func record(_ path: NWPath) {
        let physical = path.availableInterfaces
            .compactMap { interface -> (NWInterface, Int)? in
                guard let priority = Self.priority(for: interface.type) else {
                    return nil
                }
                return (interface, priority)
            }
            .sorted {
                $0.1 == $1.1
                    ? $0.0.name < $1.0.name
                    : $0.1 < $1.1
            }
            .map(\.0)
        let interfaces = physical
            .map { "\($0.name):\(Self.name(for: $0.type))" }
            .joined(separator: ",")
        let signature = "\(Self.name(for: path.status))|\(interfaces)"
        let interfaceIndex = physical.first.map {
            if_nametoindex($0.name)
        } ?? 0
        let continuation = state.withLock {
            state -> CheckedContinuation<Void, Never>? in
            guard !state.stopped else {
                return nil
            }
            if state.signature != signature
                || state.physicalInterfaceIndex != interfaceIndex
            {
                state.signature = signature
                state.physicalInterfaceIndex = interfaceIndex
                state.generation &+= 1
            }
            let continuation = state.continuation
            state.continuation = nil
            return continuation
        }
        continuation?.resume()
    }

    private func finishInitialWait() {
        let continuation = state.withLock {
            state -> CheckedContinuation<Void, Never>? in
            let continuation = state.continuation
            state.continuation = nil
            return continuation
        }
        continuation?.resume()
    }

    private static func priority(
        for type: NWInterface.InterfaceType
    ) -> Int? {
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

    private static func name(for status: NWPath.Status) -> String {
        switch status {
        case .satisfied:
            "satisfied"
        case .unsatisfied:
            "unsatisfied"
        case .requiresConnection:
            "requires-connection"
        @unknown default:
            "unknown"
        }
    }

    private static func name(for type: NWInterface.InterfaceType) -> String {
        switch type {
        case .wifi:
            "wifi"
        case .cellular:
            "cellular"
        case .wiredEthernet:
            "ethernet"
        case .loopback:
            "loopback"
        case .other:
            "other"
        @unknown default:
            "unknown"
        }
    }
}
