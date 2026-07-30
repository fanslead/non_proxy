import Network
import NetworkExtension

enum DirectRelayStartResult {
    case accepted
    case physicalInterfaceUnavailable
    case capacityExceeded
}

final class DirectFlowRelayCoordinator: Sendable {
    private let interfaces: PhysicalInterfaceCatalog
    private let registry: FlowRelayRegistry

    init(
        interfaces: PhysicalInterfaceCatalog,
        registry: FlowRelayRegistry
    ) {
        self.interfaces = interfaces
        self.registry = registry
    }

    func startTCP(
        flow: NEAppProxyTCPFlow,
        endpoint: NWEndpoint
    ) -> DirectRelayStartResult {
        guard let interface = interfaces.preferredInterface() else {
            return .physicalInterfaceUnavailable
        }
        let connection = DirectConnectionFactory.makeTCP(
            endpoint: endpoint,
            interface: interface
        )
        let queue = DispatchQueue(
            label: "com.nonproxy.transparent.tcp.\(UUID().uuidString)"
        )
        let registry = registry
        let relay = DirectTCPFlowRelay(
            flow: flow,
            connection: connection,
            budget: registry,
            queue: queue,
            onFinish: { relay in
                registry.remove(relay)
            }
        )
        guard registry.insert(relay) else {
            return .capacityExceeded
        }
        relay.start()
        return .accepted
    }

    func startUDP(flow: NEAppProxyUDPFlow) -> DirectRelayStartResult {
        guard let interface = interfaces.preferredInterface() else {
            return .physicalInterfaceUnavailable
        }
        let queue = DispatchQueue(
            label: "com.nonproxy.transparent.udp.\(UUID().uuidString)"
        )
        let registry = registry
        let relay = DirectUDPFlowRelay(
            flow: flow,
            interface: interface,
            budget: registry,
            queue: queue,
            onFinish: { relay in
                registry.remove(relay)
            }
        )
        guard registry.insert(relay) else {
            return .capacityExceeded
        }
        relay.start()
        return .accepted
    }
}
