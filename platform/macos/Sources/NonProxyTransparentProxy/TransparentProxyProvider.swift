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
            guard let self else {
                completion.complete(
                    with: ProviderError.lifecycle("Transparent Provider 已释放")
                )
                return
            }
            do {
                let paths = try MacProviderPaths.live()
                let rejectedFlows = self.rejectedFlows
                let components = try MacProviderBootstrap.make(
                    kind: .transparentProxy,
                    paths: paths,
                    metricsReader: {
                        ProviderHealthMetrics(
                            activeFlowCount: rejectedFlows.activeFlowCount
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
                guard self.providerState.install(components, runID: runID) else {
                    components.lifecycle.stop()
                    throw CancellationError()
                }
                completion.complete(with: nil)
            } catch {
                self.providerState.failStart(runID: runID)
                completion.complete(with: error)
            }
        }
    }

    public override func stopProxy(
        with reason: NEProviderStopReason,
        completionHandler: @escaping () -> Void
    ) {
        providerState.remove()?.lifecycle.stop()
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
        guard let components = providerState.runtimeComponents() else {
            return reject(flow, code: "NP_PROVIDER_NOT_READY")
        }
        do {
            let context = try contextFactory.make(
                flow: flow,
                endpoint: endpoint,
                transport: transport
            )
            let decision = try components.runtime.decide(context: context)
            switch TransparentFlowPlanner.plan(
                decision: decision,
                proxyRelayAvailable: false
            ) {
            case .direct:
                return false
            case .proxy:
                return reject(flow, code: "NP_PROXY_RELAY_UNAVAILABLE")
            case .reject(let errorCode):
                return reject(flow, code: errorCode)
            }
        } catch let error as ProviderError {
            return reject(flow, code: error.code)
        } catch {
            return reject(flow, code: "NP_FLOW_DECISION_FAILED")
        }
    }

    private func reject(_ flow: NEAppProxyFlow, code: String) -> Bool {
        rejectedFlows.reject(flow, errorCode: code)
        return true
    }
}
