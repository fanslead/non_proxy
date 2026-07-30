import Foundation
import Network
import NetworkExtension
import NonProxyProviderCore

enum ProxyRelayStartResult {
    case accepted
    case invalidEndpoint
    case capacityExceeded
}

final class ProxyFlowRelayCoordinator: Sendable {
    private let socketPath: String
    private let capability: Data
    private let registry: FlowRelayRegistry

    init(
        socketPath: String,
        capability: Data,
        registry: FlowRelayRegistry
    ) throws {
        guard socketPath.hasPrefix("/"),
              !socketPath.contains("\0"),
              capability.count == NPF1PayloadCodec.capabilityBytes
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        self.socketPath = socketPath
        self.capability = capability
        self.registry = registry
    }

    func startTCP(
        flow: NEAppProxyTCPFlow,
        destination: PolicyDestination,
        outboundID: String
    ) -> ProxyRelayStartResult {
        let endpoint: NPF1Endpoint
        do {
            endpoint = try ProxyFlowEndpointCodec.encode(
                destination: destination
            )
        } catch {
            return .invalidEndpoint
        }
        let queue = DispatchQueue(
            label: "com.nonproxy.proxy.tcp.\(UUID().uuidString)"
        )
        let registry = registry
        let relay = ProxyTCPFlowRelay(
            flow: flow,
            socketPath: socketPath,
            capability: capability,
            outboundID: outboundID,
            endpoint: endpoint,
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

    func startUDP(
        flow: NEAppProxyUDPFlow,
        destination: PolicyDestination,
        outboundID: String
    ) -> ProxyRelayStartResult {
        let endpoint: NPF1Endpoint
        do {
            endpoint = try ProxyFlowEndpointCodec.encode(
                destination: destination
            )
        } catch {
            return .invalidEndpoint
        }
        let queue = DispatchQueue(
            label: "com.nonproxy.proxy.udp.\(UUID().uuidString)"
        )
        let registry = registry
        let relay = ProxyUDPFlowRelay(
            flow: flow,
            socketPath: socketPath,
            capability: capability,
            outboundID: outboundID,
            initialEndpoint: endpoint,
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
