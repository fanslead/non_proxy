import Foundation
import Network
import NetworkExtension
import NonProxyMacPlatformSupport
import NonProxyProviderCore

@objc(DNSProxyProvider)
public final class DNSProxyProvider:
    NEDNSProxyProvider,
    NEAppProxyUDPFlowHandling,
    @unchecked Sendable
{
    private let providerState = DNSProviderState()
    private let relays = DNSFlowRelayRegistry()
    private let identityResolver = MacAppIdentityResolver()
    private let settingsObservation = DNSSettingsObservationStore()

    public override func startProxy(
        options: [String: Any]? = nil,
        completionHandler: @escaping (Error?) -> Void
    ) {
        let completion = DNSProviderStartCompletion(completionHandler)
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
                    with: ProviderError.lifecycle("DNS Provider 已释放")
                )
                return
            }
            var components: MacProviderRuntimeComponents?
            var networkProfile: DNSNetworkProfileMonitor?
            do {
                let paths = try MacProviderPaths.live()
                let relays = self.relays
                let created = try MacProviderBootstrap.make(
                    kind: .dnsProxy,
                    paths: paths,
                    metricsReader: {
                        ProviderHealthMetrics(
                            activeFlowCount: relays.activeFlowCount,
                            queuedBytes: 0
                        )
                    }
                )
                components = created
                try await created.lifecycle.start()

                let catalogs = DNSResolverCatalogStore(
                    DNSSystemResolverCatalog(
                        settings: self.systemDNSSettings ?? []
                    )
                )
                let monitor = DNSNetworkProfileMonitor()
                networkProfile = monitor
                await monitor.start()
                let runtime = DNSProviderRuntime(
                    provider: created,
                    catalogs: catalogs,
                    networkProfile: monitor,
                    coordinator: DNSQueryCoordinator(
                        runtime: created.runtime,
                        resolver: created.control,
                        catalogs: catalogs,
                        networkProfile: monitor
                    )
                )
                self.relays.beginAccepting()
                guard self.providerState.install(runtime, runID: runID) else {
                    throw CancellationError()
                }
                self.observeSystemDNSSettings(catalogs: catalogs)
                completion.complete(with: nil)
            } catch {
                self.relays.stopAcceptingAndCancelAll()
                networkProfile?.stop()
                components?.lifecycle.stop()
                components?.control.shutdown()
                self.providerState.failStart(runID: runID)
                completion.complete(with: error)
            }
        }
    }

    public override func stopProxy(
        with reason: NEProviderStopReason,
        completionHandler: @escaping () -> Void
    ) {
        settingsObservation.invalidate()
        let runtime = providerState.remove()
        relays.stopAcceptingAndCancelAll()
        runtime?.networkProfile.stop()
        runtime?.provider.lifecycle.stop()
        runtime?.provider.control.shutdown()
        completionHandler()
    }

    public override func handleNewFlow(_ flow: NEAppProxyFlow) -> Bool {
        if let udpFlow = flow as? NEAppProxyUDPFlow {
            return accept(udpFlow)
        }
        if let tcpFlow = flow as? NEAppProxyTCPFlow {
            return accept(tcpFlow)
        }
        return false
    }

    public func handleNewUDPFlow(
        _ flow: NEAppProxyUDPFlow,
        initialRemoteFlowEndpoint remoteEndpoint: NWEndpoint
    ) -> Bool {
        accept(flow)
    }

    private func accept(_ flow: NEAppProxyUDPFlow) -> Bool {
        guard let runtime = providerState.runtime() else {
            return false
        }
        runtime.catalogs.replace(
            with: DNSSystemResolverCatalog(
                settings: systemDNSSettings ?? []
            )
        )
        let relay = DNSUDPFlowRelay(
            flow: flow,
            app: identityResolver.resolve(metadata: flow.metaData),
            coordinator: runtime.coordinator,
            onFinish: { [weak relays] id in
                relays?.remove(id: id)
            }
        )
        guard relays.insert(relay) else {
            return false
        }
        relay.start()
        return true
    }

    private func accept(_ flow: NEAppProxyTCPFlow) -> Bool {
        guard let runtime = providerState.runtime() else {
            return false
        }
        runtime.catalogs.replace(
            with: DNSSystemResolverCatalog(
                settings: systemDNSSettings ?? []
            )
        )
        let relay = DNSTCPFlowRelay(
            flow: flow,
            app: identityResolver.resolve(metadata: flow.metaData),
            coordinator: runtime.coordinator,
            onFinish: { [weak relays] id in
                relays?.remove(id: id)
            }
        )
        guard relays.insert(relay) else {
            return false
        }
        relay.start()
        return true
    }

    private func observeSystemDNSSettings(
        catalogs: DNSResolverCatalogStore
    ) {
        let observation = observe(
            \.systemDNSSettings,
            options: [.new]
        ) { _, change in
            catalogs.replace(
                with: DNSSystemResolverCatalog(
                    settings: change.newValue.flatMap { $0 } ?? []
                )
            )
        }
        settingsObservation.replace(with: observation)
    }
}
