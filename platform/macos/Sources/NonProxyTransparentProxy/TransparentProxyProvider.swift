import Network
import NetworkExtension
import NonProxyMacPlatformSupport
import NonProxyProviderContracts
import NonProxyProviderCore

@objc(TransparentProxyProvider)
public final class TransparentProxyProvider:
    NETransparentProxyProvider,
    NEAppProxyUDPFlowHandling,
    @unchecked Sendable
{
    private let providerState = TransparentProviderState()
    private let rejectedFlows = RejectedFlowRegistry()
    private let flowRelays = FlowRelayRegistry()
    private let contextFactory = MacFlowContextFactory()

    public override func startProxy(
        options: [String: Any]? = nil,
        completionHandler: @escaping (Error?) -> Void
    ) {
        let completion = ProviderStartCompletion(completionHandler)
        let runID: UUID
        do {
            runID = try providerState.beginStart()
        } catch {
            completion.complete(with: error)
            return
        }
        Task { [weak self] in
            var startedInterfaces: PhysicalInterfaceCatalog?
            guard let self else {
                completion.complete(
                    with: ProviderError.lifecycle("Transparent Provider 已释放")
                )
                return
            }
            do {
                let paths = try MacProviderPaths.live()
                let interfaces = PhysicalInterfaceCatalog()
                startedInterfaces = interfaces
                await interfaces.start()
                let rejectedFlows = self.rejectedFlows
                let flowRelays = self.flowRelays
                let components = try MacProviderBootstrap.make(
                    kind: .transparentProxy,
                    paths: paths,
                    metricsReader: {
                        ProviderHealthMetrics(
                            activeFlowCount: rejectedFlows.activeFlowCount
                                + flowRelays.activeFlowCount,
                            queuedBytes: flowRelays.queuedBytes
                        )
                    }
                )
                try await components.lifecycle.start()
                do {
                    try await self.setTunnelNetworkSettings(
                        TransparentNetworkSettingsFactory.make()
                    )
                } catch {
                    components.lifecycle.stop()
                    throw error
                }
                let runtime = TransparentProviderRuntime(
                    provider: components,
                    interfaces: interfaces,
                    directRelays: DirectFlowRelayCoordinator(
                        interfaces: interfaces,
                        registry: flowRelays
                    ),
                    proxyRelays: try ProxyFlowRelayCoordinator(
                        socketPath: paths.flowSocketPath,
                        capability: paths.readBootstrapCapability(),
                        registry: flowRelays
                    )
                )
                flowRelays.beginAccepting()
                guard self.providerState.install(runtime, runID: runID) else {
                    flowRelays.stopAcceptingAndCancelAll()
                    components.lifecycle.stop()
                    throw CancellationError()
                }
                completion.complete(with: nil)
            } catch {
                startedInterfaces?.stop()
                self.providerState.failStart(runID: runID)
                completion.complete(with: error)
            }
        }
    }

    public override func stopProxy(
        with reason: NEProviderStopReason,
        completionHandler: @escaping () -> Void
    ) {
        let runtime = providerState.remove()
        runtime?.provider.lifecycle.stop()
        runtime?.interfaces.stop()
        flowRelays.stopAcceptingAndCancelAll()
        rejectedFlows.closeAll()
        completionHandler()
    }

    public override func handleNewFlow(_ flow: NEAppProxyFlow) -> Bool {
        guard let tcpFlow = flow as? NEAppProxyTCPFlow else {
            return reject(flow, code: "NP_FLOW_TYPE_UNSUPPORTED")
        }
        return handle(
            flow,
            endpoint: tcpFlow.remoteFlowEndpoint,
            transport: .tcp
        )
    }

    public func handleNewUDPFlow(
        _ flow: NEAppProxyUDPFlow,
        initialRemoteFlowEndpoint remoteEndpoint: NWEndpoint
    ) -> Bool {
        handle(flow, endpoint: remoteEndpoint, transport: .udp)
    }

    private func handle(
        _ flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol
    ) -> Bool {
        guard let runtime = providerState.runtime() else {
            return reject(flow, code: "NP_PROVIDER_NOT_READY")
        }
        do {
            let context = try contextFactory.make(
                flow: flow,
                endpoint: endpoint,
                transport: transport
            )
            let decision = try runtime.provider.runtime.decide(context: context)
            switch TransparentFlowPlanner.plan(
                decision: decision,
                proxyRelayAvailable: true
            ) {
            case .direct:
                return startDirectRelay(
                    runtime: runtime,
                    flow: flow,
                    endpoint: endpoint,
                    transport: transport
                )
            case .proxy(let outboundID):
                return startProxyRelay(
                    runtime: runtime,
                    flow: flow,
                    destination: context.destination,
                    transport: transport,
                    outboundID: outboundID
                )
            case .reject(let errorCode):
                return reject(flow, code: errorCode)
            }
        } catch let error as ProviderError {
            return reject(flow, code: error.code)
        } catch {
            return reject(flow, code: "NP_FLOW_DECISION_FAILED")
        }
    }

    private func startProxyRelay(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        destination: PolicyDestination,
        transport: Nonproxy_Common_V1_TransportProtocol,
        outboundID: String
    ) -> Bool {
        let result: ProxyRelayStartResult
        switch transport {
        case .tcp:
            result = (flow as? NEAppProxyTCPFlow).map {
                runtime.proxyRelays.startTCP(
                    flow: $0,
                    destination: destination,
                    outboundID: outboundID
                )
            } ?? .invalidEndpoint
        case .udp:
            result = (flow as? NEAppProxyUDPFlow).map {
                runtime.proxyRelays.startUDP(
                    flow: $0,
                    destination: destination,
                    outboundID: outboundID
                )
            } ?? .invalidEndpoint
        default:
            result = .invalidEndpoint
        }
        switch result {
        case .accepted:
            return true
        case .invalidEndpoint:
            return reject(flow, code: "NP_PROXY_ENDPOINT_INVALID")
        case .capacityExceeded:
            return reject(flow, code: "NP_PROXY_RELAY_CAPACITY_EXCEEDED")
        }
    }

    private func startDirectRelay(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol
    ) -> Bool {
        let result: DirectRelayStartResult
        switch transport {
        case .tcp:
            result = (flow as? NEAppProxyTCPFlow).map {
                runtime.directRelays.startTCP(flow: $0, endpoint: endpoint)
            } ?? .capacityExceeded
        case .udp:
            result = (flow as? NEAppProxyUDPFlow).map {
                runtime.directRelays.startUDP(flow: $0)
            } ?? .capacityExceeded
        default:
            result = .capacityExceeded
        }
        switch result {
        case .accepted:
            return true
        case .physicalInterfaceUnavailable:
            return reject(
                flow,
                code: "NP_DIRECT_PHYSICAL_INTERFACE_UNAVAILABLE"
            )
        case .capacityExceeded:
            return reject(flow, code: "NP_DIRECT_RELAY_CAPACITY_EXCEEDED")
        }
    }

    private func reject(_ flow: NEAppProxyFlow, code: String) -> Bool {
        rejectedFlows.reject(flow, errorCode: code)
        return true
    }
}
