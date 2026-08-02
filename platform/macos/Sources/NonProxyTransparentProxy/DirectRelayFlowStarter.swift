import Network
import NetworkExtension
import NonProxyMacNetworkIdentity
import NonProxyProviderContracts
import NonProxyProviderCore

enum DirectRelayFlowStarter {
    static func start(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol,
        observation: ProviderDecisionObservation,
        network: MacNetworkEnvironmentSnapshot,
        failOpen: Bool,
        rejectedFlows: RejectedFlowRegistry
    ) -> Bool {
        let onEstablished: @Sendable (String) -> Void = { interface in
            TransparentDecisionReporter.report(
                runtime: runtime,
                observation: observation,
                path: .direct(interfaceName: interface, failOpen: failOpen),
                errorCode: failOpen ? "NP_PROXY_FAIL_OPEN_DIRECT" : nil
            )
        }
        let onSetupFailed: @Sendable (String) -> Void = { code in
            TransparentDecisionReporter.report(
                runtime: runtime,
                observation: observation,
                path: .decision,
                errorCode: code
            )
        }
        let result = startRelay(
            runtime: runtime,
            flow: flow,
            endpoint: endpoint,
            transport: transport,
            network: network,
            onEstablished: onEstablished,
            onSetupFailed: onSetupFailed
        )
        return handle(
            result,
            runtime: runtime,
            flow: flow,
            observation: observation,
            rejectedFlows: rejectedFlows
        )
    }

    private static func startRelay(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol,
        network: MacNetworkEnvironmentSnapshot,
        onEstablished: @escaping @Sendable (String) -> Void,
        onSetupFailed: @escaping @Sendable (String) -> Void
    ) -> DirectRelayStartResult {
        switch transport {
        case .tcp:
            (flow as? NEAppProxyTCPFlow).map {
                runtime.directRelays.startTCP(
                    flow: $0,
                    endpoint: endpoint,
                    interface: network.preferredInterface,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .capacityExceeded
        case .udp:
            (flow as? NEAppProxyUDPFlow).map {
                runtime.directRelays.startUDP(
                    flow: $0,
                    interface: network.preferredInterface,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .capacityExceeded
        default:
            .capacityExceeded
        }
    }

    private static func handle(
        _ result: DirectRelayStartResult,
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        observation: ProviderDecisionObservation,
        rejectedFlows: RejectedFlowRegistry
    ) -> Bool {
        let errorCode: String
        switch result {
        case .accepted:
            return true
        case .physicalInterfaceUnavailable:
            errorCode = "NP_DIRECT_PHYSICAL_INTERFACE_UNAVAILABLE"
        case .capacityExceeded:
            errorCode = "NP_DIRECT_RELAY_CAPACITY_EXCEEDED"
        }
        TransparentDecisionReporter.report(
            runtime: runtime,
            observation: observation,
            path: .decision,
            errorCode: errorCode
        )
        return rejectedFlows.rejectAndHandle(flow, errorCode: errorCode)
    }
}
