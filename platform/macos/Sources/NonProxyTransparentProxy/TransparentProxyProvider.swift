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
            return rejectedFlows.rejectAndHandle(
                flow,
                errorCode: "NP_FLOW_TYPE_UNSUPPORTED"
            )
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
            return rejectedFlows.rejectAndHandle(
                flow,
                errorCode: "NP_PROVIDER_NOT_READY"
            )
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
            guard case .decision(let decision) = evaluation.disposition else {
                return false
            }
            let observation = ProviderDecisionObservation(
                flowID: UUID().uuidString.lowercased(),
                context: context,
                decision: decision,
                proxyTarget: evaluation.proxyTarget,
                observedAt: observedAt,
                decisionLatencyNanoseconds: decisionFinished - decisionStarted
            )
            switch TransparentFlowPlanner.plan(
                decision: decision,
                proxyTarget: evaluation.proxyTarget,
                proxyRelayAvailable: true
            ) {
            case .direct:
                return DirectRelayFlowStarter.start(
                    runtime: runtime,
                    flow: flow,
                    endpoint: endpoint,
                    transport: transport,
                    observation: observation,
                    network: network,
                    failOpen: false,
                    rejectedFlows: rejectedFlows
                )
            case .proxy(let proxyTarget):
                return startProxyRelay(
                    runtime: runtime,
                    flow: flow,
                    endpoint: endpoint,
                    destination: context.destination,
                    transport: transport,
                    proxyTarget: proxyTarget,
                    observation: observation,
                    network: network
                )
            case .reject(let errorCode):
                TransparentDecisionReporter.report(
                    runtime: runtime,
                    observation: observation,
                    path: .decision,
                    errorCode: errorCode
                )
                return rejectedFlows.rejectAndHandle(
                    flow,
                    errorCode: errorCode
                )
            }
        } catch let error as ProviderError {
            return rejectedFlows.rejectAndHandle(flow, errorCode: error.code)
        } catch {
            return rejectedFlows.rejectAndHandle(
                flow,
                errorCode: "NP_FLOW_DECISION_FAILED"
            )
        }
    }

    private func startProxyRelay(
        runtime: TransparentProviderRuntime,
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        destination: PolicyDestination,
        transport: Nonproxy_Common_V1_TransportProtocol,
        proxyTarget: ProviderProxyTarget,
        observation: ProviderDecisionObservation,
        network: MacNetworkEnvironmentSnapshot
    ) -> Bool {
        let onEstablished: @Sendable (String) -> Void = {
            selectedOutboundID in
            TransparentDecisionReporter.report(
                runtime: runtime,
                observation: observation,
                path: .proxy(outboundID: selectedOutboundID)
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
                    proxyTarget: proxyTarget,
                    onEstablished: onEstablished,
                    onSetupFailed: onSetupFailed
                )
            } ?? .invalidEndpoint
        case .udp:
            result = (flow as? NEAppProxyUDPFlow).map {
                runtime.proxyRelays.startUDP(
                    flow: $0,
                    destination: destination,
                    proxyTarget: proxyTarget,
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
            _ = DirectRelayFlowStarter.start(
                runtime: runtime,
                flow: flow,
                endpoint: endpoint,
                transport: transport,
                observation: observation,
                network: network,
                failOpen: true,
                rejectedFlows: rejectedFlows
            )
        case .reject(let errorCode):
            TransparentDecisionReporter.report(
                runtime: runtime,
                observation: observation,
                path: .decision,
                errorCode: errorCode
            )
            _ = rejectedFlows.rejectAndHandle(flow, errorCode: errorCode)
        }
    }
}
