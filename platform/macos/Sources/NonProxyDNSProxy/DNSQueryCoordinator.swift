import Foundation
import NonProxyProviderContracts
import NonProxyProviderCore

public struct DNSFlowQueryContext: Sendable {
    public let message: Data
    public let app: PolicyAppIdentity
    public let transport: Nonproxy_Common_V1_TransportProtocol

    public init(
        message: Data,
        app: PolicyAppIdentity,
        transport: Nonproxy_Common_V1_TransportProtocol
    ) {
        self.message = message
        self.app = app
        self.transport = transport
    }
}

public final class DNSQueryCoordinator: Sendable {
    private let runtime: ProviderPolicyRuntime
    private let resolver: any ProviderDNSResolving
    private let catalogs: DNSResolverCatalogStore
    private let networkProfile: DNSNetworkProfileMonitor
    private let capacity: DNSQueryCapacity

    public init(
        runtime: ProviderPolicyRuntime,
        resolver: any ProviderDNSResolving,
        catalogs: DNSResolverCatalogStore,
        networkProfile: DNSNetworkProfileMonitor,
        capacity: DNSQueryCapacity = DNSQueryCapacity()
    ) {
        self.runtime = runtime
        self.resolver = resolver
        self.catalogs = catalogs
        self.networkProfile = networkProfile
        self.capacity = capacity
    }

    public func resolve(_ context: DNSFlowQueryContext) async throws -> Data {
        guard await capacity.acquire() else {
            throw DNSProxyError.capacityExceeded("DNS 并发查询数量超过上限")
        }
        do {
            let response = try await resolveWithCapacity(context)
            await capacity.release()
            return response
        } catch {
            await capacity.release()
            throw error
        }
    }

    private func resolveWithCapacity(
        _ context: DNSFlowQueryContext
    ) async throws -> Data {
        let question = try DNSMessageParser.parseQuery(context.message)
        let baseProfileID = networkProfile.profileID()
        let policyContext = PolicyConnectionContext(
            app: context.app,
            destination: PolicyDestination(
                normalizedDomain: DomainNameNormalizer.normalize(question.name),
                registrableDomain: nil,
                ipAddress: nil,
                transport: context.transport,
                port: 53
            ),
            networkProfileID: nil
        )
        let decision = try runtime.decide(context: policyContext)
        let plan = DNSRoutePlanner.plan(decision: decision)
        if plan == .refuse {
            return DNSResponseBuilder.refused(
                query: context.message,
                question: question
            )
        }
        let upstreams = catalogs.upstreams(for: question.name)
        guard !upstreams.isEmpty else {
            throw DNSProxyError.resolverUnavailable(
                "当前网络没有可用的明文系统 DNS 上游"
            )
        }
        let profileID = networkProfile.profileID(upstreams: upstreams)
        var request = Nonproxy_Provider_V1_ResolveDnsRequest()
        request.queryID = UUID().uuidString.lowercased()
        request.app = protobufIdentity(context.app)
        request.qname = question.name
        request.qtype = UInt32(question.type)
        request.networkProfileID = profileID.isEmpty
            ? baseProfileID
            : profileID
        request.dnsMessage = context.message
        request.upstreams = upstreams.map(\.protobuf)
        request.snapshotVersion = decision.snapshotVersion
        switch plan {
        case .direct:
            let interfaceIndex = networkProfile.preferredInterfaceIndex
            guard interfaceIndex > 0 else {
                throw DNSProxyError.resolverUnavailable(
                    "DIRECT DNS 没有可绑定的物理网卡"
                )
            }
            request.requestedRoute = .direct
            request.directInterfaceIndex = interfaceIndex
        case .proxy(let outboundID):
            request.requestedRoute = .proxy
            request.requestedOutboundID = outboundID
        case .refuse:
            throw DNSProxyError.providerUnavailable("DNS 路由计划无效")
        }

        let response = try await resolver.resolveDNS(request)
        try validate(response, request: request, question: question)
        return response.dnsMessage
    }

    private func validate(
        _ response: Nonproxy_Provider_V1_ResolveDnsResponse,
        request: Nonproxy_Provider_V1_ResolveDnsRequest,
        question: DNSQuestion
    ) throws {
        guard response.route == request.requestedRoute else {
            throw DNSProxyError.responseInvalid("DNS 响应路由标签不匹配")
        }
        if request.requestedRoute == .proxy {
            guard response.outboundID == request.requestedOutboundID else {
                throw DNSProxyError.responseInvalid(
                    "DNS 响应代理出口标签不匹配"
                )
            }
        } else if !response.outboundID.isEmpty {
            throw DNSProxyError.responseInvalid("直连 DNS 响应携带了代理出口")
        }
        try DNSMessageParser.validateResponse(
            response.dnsMessage,
            for: question
        )
    }

    private func protobufIdentity(
        _ identity: PolicyAppIdentity
    ) -> Nonproxy_Common_V1_AppIdentity {
        var value = Nonproxy_Common_V1_AppIdentity()
        value.platform = .macos
        value.stableID = identity.stableID
        value.signerID = identity.signerID ?? ""
        value.parentStableID = identity.parentStableID ?? ""
        value.helperGroupID = identity.helperGroupID ?? ""
        return value
    }
}
