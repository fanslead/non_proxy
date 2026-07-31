import Network
import NetworkExtension
import NonProxyMacNetworkIdentity
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
            var startedEnvironment: MacNetworkEnvironmentMonitor?
            guard let self else {
                completion.complete(
                    with: ProviderError.lifecycle("Transparent Provider 已释放")
                )
                return
            }
            do {
                let paths = try MacProviderPaths.live()
                let networkEnvironment = MacNetworkEnvironmentMonitor()
                startedEnvironment = networkEnvironment
                await networkEnvironment.start()
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
                    components.decisions.stop()
                    components.control.shutdown()
                    throw error
                }
                let runtime = TransparentProviderRuntime(
                    runID: runID,
                    provider: components,
                    networkEnvironment: networkEnvironment,
                    directRelays: DirectFlowRelayCoordinator(
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
                    components.decisions.stop()
                    components.control.shutdown()
                    throw CancellationError()
                }
                completion.complete(with: nil)
            } catch {
                startedEnvironment?.stop()
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
        runtime?.provider.decisions.stop()
        runtime?.provider.control.shutdown()
        runtime?.networkEnvironment.stop()
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
            let observedAt = Date()
            let unresolvedContext = try contextFactory.make(
                flow: flow,
                endpoint: endpoint,
                transport: transport
            )
            let network = runtime.networkEnvironment.snapshot()
            let decisionStarted = DispatchTime.now().uptimeNanoseconds
            let evaluation = try runtime.provider.runtime.evaluate(
                context: unresolvedContext,
                networkFingerprints: network.policyFingerprints
            )
            let decisionFinished = DispatchTime.now().uptimeNanoseconds
            let context = evaluation.context
            let decision = evaluation.decision
            let observation = ProviderDecisionObservation(
                flowID: UUID().uuidString.lowercased(),
                context: context,
                decision: decision,
                observedAt: observedAt,
                decisionLatencyNanoseconds: decisionFinished - decisionStarted
            )
            switch TransparentFlowPlanner.plan(
                decision: decision,
                proxyRelayAvailable: true
            ) {
            case .direct:
                return startDirectRelay(
                    runtime: runtime,
                    flow: flow,
                    endpoint: endpoint,
                    transport: transport,
                    observation: observation,
                    network: network,
                    failOpen: false
                )
            case .proxy(let outboundID):
                return startProxyRelay(
                    runtime: runtime,
                    flow: flow,
                    endpoint: endpoint,
                    destination: context.destination,
                    transport: transport,
                    outboundID: outboundID,
                    observation: observation,
                    network: network
                )
            case .reject(let errorCode):
                report(
                    runtime: runtime,
                    observation: observation,
                    path: .decision,
                    errorCode: errorCode
                )
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
        endpoint: NWEndpoint,
        destination: PolicyDestination,
        transport: Nonproxy_Common_V1_TransportProtocol,
        outboundID: String,
        observation: ProviderDecisionObservation,
        network: MacNetworkEnvironmentSnapshot
    ) -> Bool {
        let onEstablished: @Sendable () -> Void = { [weak self] in
            self?.report(
                runtime: runtime,
                observation: observation,
                path: .proxy(outboundID: outboundID)
            )
        }
        let flowReference = AppProxyFlowReference(flow)
        let onSetupFailed: @Sendable (String) -> Void = { [weak self] code in
            self?.handleProxySetupFailure(
                runtime: runtime,
                flow: flowReference.flow,
                endpoint: endpoint,
                transport: transport,
                observation: observation,
                network: network,
                code: code
            )
        }
        let result: ProxyRelayStartResult
        switch transport {
        case .tcp:
            result = (flow as? NEAppProxyTCPFlow).map {
                runtime.proxyRelays.startTCP(
                    flow: $0,
                    destination: destination,
                    outboundID: outboundID,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .invalidEndpoint
        case .udp:
            result = (flow as? NEAppProxyUDPFlow).map {
                runtime.proxyRelays.startUDP(
                    flow: $0,
                    destination: destination,
                    outboundID: outboundID,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .invalidEndpoint
        default:
            result = .invalidEndpoint
        }
        switch result {
        case .accepted:
            return true
        case .invalidEndpoint:
            handleProxySetupFailure(
                runtime: runtime,
                flow: flow,
                endpoint: endpoint,
                transport: transport,
                observation: observation,
                network: network,
                code: "NP_PROXY_ENDPOINT_INVALID"
            )
            return true
        case .capacityExceeded:
            handleProxySetupFailure(
                runtime: runtime,
                flow: flow,
                endpoint: endpoint,
                transport: transport,
                observation: observation,
                network: network,
                code: "NP_PROXY_RELAY_CAPACITY_EXCEEDED"
            )
            return true
        }
    }

    private func startDirectRelay(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol,
        observation: ProviderDecisionObservation,
        network: MacNetworkEnvironmentSnapshot,
        failOpen: Bool
    ) -> Bool {
        let onEstablished: @Sendable (String) -> Void = { [weak self] interface in
            self?.report(
                runtime: runtime,
                observation: observation,
                path: .direct(interfaceName: interface, failOpen: failOpen),
                errorCode: failOpen ? "NP_PROXY_FAIL_OPEN_DIRECT" : nil
            )
        }
        let onSetupFailed: @Sendable (String) -> Void = { [weak self] code in
            self?.report(
                runtime: runtime,
                observation: observation,
                path: .decision,
                errorCode: code
            )
        }
        let result: DirectRelayStartResult
        switch transport {
        case .tcp:
            result = (flow as? NEAppProxyTCPFlow).map {
                runtime.directRelays.startTCP(
                    flow: $0,
                    endpoint: endpoint,
                    interface: network.preferredInterface,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .capacityExceeded
        case .udp:
            result = (flow as? NEAppProxyUDPFlow).map {
                runtime.directRelays.startUDP(
                    flow: $0,
                    interface: network.preferredInterface,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .capacityExceeded
        default:
            result = .capacityExceeded
        }
        switch result {
        case .accepted:
            return true
        case .physicalInterfaceUnavailable:
            report(
                runtime: runtime,
                observation: observation,
                path: .decision,
                errorCode: "NP_DIRECT_PHYSICAL_INTERFACE_UNAVAILABLE"
            )
            return reject(
                flow,
                code: "NP_DIRECT_PHYSICAL_INTERFACE_UNAVAILABLE"
            )
        case .capacityExceeded:
            report(
                runtime: runtime,
                observation: observation,
                path: .decision,
                errorCode: "NP_DIRECT_RELAY_CAPACITY_EXCEEDED"
            )
            return reject(flow, code: "NP_DIRECT_RELAY_CAPACITY_EXCEEDED")
        }
    }

    private func handleProxySetupFailure(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol,
        observation: ProviderDecisionObservation,
        network: MacNetworkEnvironmentSnapshot,
        code: String
    ) {
        guard providerState.isCurrentStart(runID: runtime.runID) else {
            return
        }
        switch ProxySetupRecoveryPlanner.plan(
            decision: observation.decision,
            errorCode: code
        ) {
        case .directFallback:
            _ = startDirectRelay(
                runtime: runtime,
                flow: flow,
                endpoint: endpoint,
                transport: transport,
                observation: observation,
                network: network,
                failOpen: true
            )
        case .reject(let errorCode):
            report(
                runtime: runtime,
                observation: observation,
                path: .decision,
                errorCode: errorCode
            )
            _ = reject(flow, code: errorCode)
        }
    }

    private func report(
        runtime: TransparentProviderRuntime,
        observation: ProviderDecisionObservation,
        path: ProviderObservedPath,
        errorCode: String? = nil
    ) {
        guard let record = try? observation.record(
            path: path,
            errorCode: errorCode
        ) else {
            runtime.provider.decisions.recordUnreportable()
            return
        }
        runtime.provider.decisions.submit(record)
    }

    private func reject(_ flow: NEAppProxyFlow, code: String) -> Bool {
        rejectedFlows.reject(flow, errorCode: code)
        return true
    }
}
