import CoreWLAN
import Darwin
import Foundation
import Network
import Synchronization
import SystemConfiguration

public final class MacNetworkEnvironmentMonitor: @unchecked Sendable {
    public typealias WiFiSSIDFetcher = @Sendable (
        String,
        @escaping @Sendable (Data?) -> Void
    ) -> Void
    public typealias DefaultGatewayReader = @Sendable (
        String
    ) -> MacNetworkFingerprint?

    private static let initialPathTimeout: DispatchTimeInterval = .seconds(3)

    private struct State {
        var interfaces: [NWInterface] = []
        var signature = "unknown"
        var generation: UInt64 = 1
        var fingerprints: [MacNetworkFingerprint]
        var continuation: CheckedContinuation<Void, Never>?
        var refreshToken: UInt64 = 0
        var started = false
        var stopped = false
        var physicalInterfaceIndex: UInt32

        init(
            physicalInterfaceIndex: UInt32,
            fingerprints: [MacNetworkFingerprint]
        ) {
            self.physicalInterfaceIndex = physicalInterfaceIndex
            self.fingerprints = fingerprints
        }
    }

    private let runID = UUID()
    private let monitor: NWPathMonitor
    private let wifiSSIDFetcher: WiFiSSIDFetcher
    private let defaultGatewayReader: DefaultGatewayReader
    private let queue = DispatchQueue(
        label: "com.nonproxy.network-environment"
    )
    private let state: Mutex<State>

    public init(
        monitor: NWPathMonitor = NWPathMonitor(),
        initialInterfaceIndex: UInt32 = 0,
        initialFingerprints: [MacNetworkFingerprint] = [],
        wifiSSIDFetcher: WiFiSSIDFetcher? = nil,
        defaultGatewayReader: DefaultGatewayReader? = nil
    ) {
        self.monitor = monitor
        self.wifiSSIDFetcher = wifiSSIDFetcher ?? Self.fetchCurrentSSID
        self.defaultGatewayReader = defaultGatewayReader
            ?? Self.readDefaultGateway
        self.state = Mutex(
            State(
                physicalInterfaceIndex: initialInterfaceIndex,
                fingerprints: initialFingerprints
            )
        )
    }

    public func start() async {
        await withCheckedContinuation { continuation in
            let shouldStart = state.withLock { state -> Bool in
                guard !state.started, !state.stopped else {
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
        let continuation = state.withLock {
            state -> CheckedContinuation<Void, Never>? in
            guard !state.stopped else {
                return nil
            }
            state.stopped = true
            state.refreshToken &+= 1
            state.interfaces = []
            state.fingerprints = []
            state.physicalInterfaceIndex = 0
            let current = state.continuation
            state.continuation = nil
            return current
        }
        monitor.cancel()
        continuation?.resume()
    }

    public func snapshot() -> MacNetworkEnvironmentSnapshot {
        state.withLock {
            MacNetworkEnvironmentSnapshot(
                fingerprints: $0.fingerprints,
                preferredInterface: $0.interfaces.first,
                preferredInterfaceIndex: $0.physicalInterfaceIndex,
                runID: runID,
                generation: $0.generation,
                signature: $0.signature
            )
        }
    }

    public static func priority(
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

    private func record(_ path: NWPath) {
        let candidates = path.availableInterfaces
            .compactMap { interface -> (
                interface: NWInterface,
                gateway: MacNetworkFingerprint?,
                rank: MacNetworkInterfaceRank
            )? in
                guard let priority = Self.priority(for: interface.type) else {
                    return nil
                }
                let gateway = defaultGatewayReader(interface.name)
                return (
                    interface,
                    gateway,
                    MacNetworkInterfaceRank(
                        isUsed: path.usesInterfaceType(interface.type),
                        priority: priority,
                        hasDefaultGateway: gateway != nil,
                        name: interface.name
                    )
                )
            }
            .sorted { $0.rank < $1.rank }
        let interfaces = candidates.map(\.interface)
        let preferred = candidates.first
        let interfaceClass = preferred.map {
            MacNetworkFingerprintFactory.name(for: $0.interface.type)
        } ?? (path.usesInterfaceType(.other) ? "other" : nil)
        let baseFingerprints = [
            preferred?.gateway,
            interfaceClass.flatMap(
                MacNetworkFingerprintFactory.interfaceClass
            ),
        ].compactMap { $0 }
        let interfaceIndex = preferred.map {
            if_nametoindex($0.interface.name)
        } ?? 0
        let signature = Self.signature(
            status: path.status,
            interfaces: interfaces,
            interfaceClass: interfaceClass
        )
        let update = state.withLock { state -> (
            token: UInt64?,
            continuation: CheckedContinuation<Void, Never>?
        ) in
            guard !state.stopped else {
                return (nil, nil)
            }
            state.refreshToken &+= 1
            if state.signature != signature
                || state.physicalInterfaceIndex != interfaceIndex
                || state.fingerprints != baseFingerprints
            {
                state.generation &+= 1
            }
            state.signature = signature
            state.interfaces = interfaces
            state.physicalInterfaceIndex = interfaceIndex
            state.fingerprints = baseFingerprints
            let continuation: CheckedContinuation<Void, Never>?
            if interfaceClass == "wifi" {
                continuation = nil
            } else {
                continuation = state.continuation
                state.continuation = nil
            }
            return (state.refreshToken, continuation)
        }
        update.continuation?.resume()
        guard interfaceClass == "wifi", let token = update.token else {
            return
        }
        wifiSSIDFetcher(preferred?.interface.name ?? "") { [weak self] ssid in
            self?.recordSSID(ssid, token: token)
        }
    }

    private func recordSSID(_ ssid: Data?, token: UInt64) {
        let fingerprint = ssid.flatMap {
            MacNetworkFingerprintFactory.wifiSSIDData($0)
        }
        let continuation = state.withLock {
            state -> CheckedContinuation<Void, Never>? in
            guard !state.stopped, state.refreshToken == token else {
                return nil
            }
            if let fingerprint,
               !state.fingerprints.contains(fingerprint)
            {
                state.fingerprints.insert(fingerprint, at: 0)
                state.generation &+= 1
            }
            let current = state.continuation
            state.continuation = nil
            return current
        }
        continuation?.resume()
    }

    private func finishInitialWait() {
        let continuation = state.withLock {
            state -> CheckedContinuation<Void, Never>? in
            let current = state.continuation
            state.continuation = nil
            return current
        }
        continuation?.resume()
    }

    private static func signature(
        status: NWPath.Status,
        interfaces: [NWInterface],
        interfaceClass: String?
    ) -> String {
        let names = interfaces.map {
            "\($0.name):\(MacNetworkFingerprintFactory.name(for: $0.type))"
        }.joined(separator: ",")
        return "\(name(for: status))|\(interfaceClass ?? "none")|\(names)"
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

    private static func fetchCurrentSSID(
        interfaceName: String,
        completion: @escaping @Sendable (Data?) -> Void
    ) {
        let interface = CWWiFiClient.shared().interface(
            withName: interfaceName
        )
        completion(interface?.ssidData())
    }

    private static func readDefaultGateway(
        interfaceName: String
    ) -> MacNetworkFingerprint? {
        for family in ["IPv4", "IPv6"] {
            let pattern = "State:/Network/Service/.*/\(family)" as CFString
            let keys = SCDynamicStoreCopyKeyList(nil, pattern) as? [String]
            for key in (keys ?? []).sorted() {
                guard let value = SCDynamicStoreCopyValue(
                    nil,
                    key as CFString
                ) as? [String: Any],
                    value["InterfaceName"] as? String == interfaceName,
                    let router = value["Router"] as? String
                else {
                    continue
                }
                let hardwareAddress = value[
                    "ARPResolvedHardwareAddress"
                ] as? String
                if let fingerprint = MacNetworkFingerprintFactory
                    .defaultGateway(
                        router,
                        hardwareAddress: hardwareAddress
                    )
                {
                    return fingerprint
                }
            }
        }
        return nil
    }
}
