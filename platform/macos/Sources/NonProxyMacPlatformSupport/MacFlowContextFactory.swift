import Network
import NetworkExtension
import NonProxyProviderContracts
import NonProxyProviderCore

public struct MacFlowContextFactory: Sendable {
    private let identityResolver: MacAppIdentityResolver

    public init(identityResolver: MacAppIdentityResolver = .init()) {
        self.identityResolver = identityResolver
    }

    public func make(
        flow: NEAppProxyFlow,
        endpoint: NWEndpoint,
        transport: Nonproxy_Common_V1_TransportProtocol,
        networkProfileID: String? = nil
    ) throws -> PolicyConnectionContext {
        let descriptor = try MacEndpointDescriptor(
            endpoint: endpoint,
            connectByName: flow.remoteHostname
        )
        return PolicyConnectionContext(
            app: identityResolver.resolve(metadata: flow.metaData),
            destination: descriptor.policyDestination(transport: transport),
            networkProfileID: networkProfileID
        )
    }
}
