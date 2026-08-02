import Foundation
import NonProxyMacNetworkIdentity
import NonProxyMacPlatformSupport
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
    private let networkEnvironment: MacNetworkEnvironmentMonitor
    private let capacity: DNSQueryCapacity
    private let decisions: any ProviderDecisionSubmitting

    public init(
        runtime: ProviderPolicyRuntime,
        resolver: any ProviderDNSResolving,
        catalogs: DNSResolverCatalogStore,
        networkEnvironment: MacNetworkEnvironmentMonitor,
        decisions: any ProviderDecisionSubmitting,
        capacity: DNSQueryCapacity = DNSQueryCapacity()
    ) {
        self.runtime = runtime
        self.resolver = resolver
        self.catalogs = catalogs
        self.networkEnvironment = networkEnvironment
        self.decisions = decisions
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
        let observedAt = Date()
        let question = try DNSMessageParser.parseQuery(context.message)
        let network = networkEnvironment.snapshot()
        let baseProfileID = network.dnsCachePartitionID()
        let unresolvedContext = PolicyConnectionContext(
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
        let decisionStarted = DispatchTime.now().uptimeNanoseconds
        let evaluation = try runtime.evaluate(
            context: unresolvedContext,
            networkFingerprints: network.policyFingerprints
        )
        let decisionFinished = DispatchTime.now().uptimeNanoseconds
        let policyContext = evaluation.context
        if case .bypass(let snapshotVersion, _) = evaluation.disposition {
            return try await resolveSystem(
                context: context,
                question: question,
                network: network,
                baseProfileID: baseProfileID,
                snapshotVersion: snapshotVersion
            )
        }
        guard case .decision(let decision) = evaluation.disposition else {
            throw DNSProxyError.providerUnavailable("DNS 运行态判定无效")
        }
        let observation = ProviderDecisionObservation(
            flowID: UUID().uuidString.lowercased(),
            context: policyContext,
            decision: decision,
            proxyTarget: evaluation.proxyTarget,
            observedAt: observedAt,
            decisionLatencyNanoseconds: decisionFinished - decisionStarted
        )
        let plan = DNSRoutePlanner.plan(
            decision: decision,
            proxyTarget: evaluation.proxyTarget
        )
        if plan == .refuse {
            report(observation, path: .decision)
            return DNSResponseBuilder.refused(
                query: context.message,
                question: question
            )
        }
        let upstreams = catalogs.upstreams(for: question.name)
        guard !upstreams.isEmpty else {
            report(
                observation,
                path: .decision,
                errorCode: "NP_DNS_UPSTREAM_UNAVAILABLE"
            )
            throw DNSProxyError.resolverUnavailable(
                "当前网络没有可用的明文系统 DNS 上游"
            )
        }
        let profileID = network.dnsCachePartitionID(
            resolverKeys: upstreams.map {
                "\($0.ipAddress):\($0.port):\($0.scopeID)"
            }
        )
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
            let interfaceIndex = network.preferredInterfaceIndex
            guard interfaceIndex > 0 else {
                report(
                    observation,
                    path: .decision,
                    errorCode: "NP_DNS_DIRECT_INTERFACE_UNAVAILABLE"
                )
                throw DNSProxyError.resolverUnavailable(
                    "DIRECT DNS 没有可绑定的物理网卡"
                )
            }
            request.requestedRoute = .direct
            request.directInterfaceIndex = interfaceIndex
            return try await resolveDirect(
                request,
                question: question,
                observation: observation
            )
        case .proxy(let proxyTarget):
            request.requestedRoute = .proxy
            switch proxyTarget {
            case .outbound(let id):
                request.requestedOutboundID = id
            case .group(let id, let snapshotVersion, _):
                guard snapshotVersion == request.snapshotVersion else {
                    throw DNSProxyError.providerUnavailable(
                        "DNS 出口组快照版本不匹配"
                    )
                }
                request.requestedOutboundGroupID = id
            }
            return try await resolveProxy(
                request,
                question: question,
                observation: observation,
                proxyTarget: proxyTarget,
                network: network
            )
        case .refuse:
            throw DNSProxyError.providerUnavailable("DNS 路由计划无效")
        }
    }

    private func resolveSystem(
        context: DNSFlowQueryContext,
        question: DNSQuestion,
        network: MacNetworkEnvironmentSnapshot,
        baseProfileID: String,
        snapshotVersion: UInt64
    ) async throws -> Data {
        let upstreams = catalogs.upstreams(for: question.name)
        guard !upstreams.isEmpty else {
            throw DNSProxyError.resolverUnavailable(
                "当前网络没有可用的明文系统 DNS 上游"
            )
        }
        let profileID = network.dnsCachePartitionID(
            resolverKeys: upstreams.map {
                "\($0.ipAddress):\($0.port):\($0.scopeID)"
            }
        )
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
        request.snapshotVersion = snapshotVersion
        request.requestedRoute = .system
        let response = try await resolver.resolveDNS(request)
        try DNSResponseValidator.validate(
            response,
            request: request,
            question: question
        )
        return response.dnsMessage
    }

    private func resolveDirect(
        _ request: Nonproxy_Provider_V1_ResolveDnsRequest,
        question: DNSQuestion,
        observation: ProviderDecisionObservation
    ) async throws -> Data {
        do {
            let response = try await resolver.resolveDNS(request)
            try DNSResponseValidator.validate(
                response,
                request: request,
                question: question
            )
            if response.cacheHit {
                report(observation, path: .decision)
            } else {
                report(
                    observation,
                    path: .direct(
                        interfaceName: "ifindex:\(request.directInterfaceIndex)",
                        failOpen: false
                    )
                )
            }
            return response.dnsMessage
        } catch {
            report(
                observation,
                path: .decision,
                errorCode: "NP_DNS_DIRECT_RESOLVE_FAILED"
            )
            throw error
        }
    }

    private func resolveProxy(
        _ request: Nonproxy_Provider_V1_ResolveDnsRequest,
        question: DNSQuestion,
        observation: ProviderDecisionObservation,
        proxyTarget: ProviderProxyTarget,
        network: MacNetworkEnvironmentSnapshot
    ) async throws -> Data {
        do {
            let response = try await resolver.resolveDNS(request)
            try DNSResponseValidator.validate(
                response,
                request: request,
                question: question,
                proxyTarget: proxyTarget
            )
            if response.cacheHit {
                report(observation, path: .decision)
            } else {
                report(
                    observation,
                    path: .proxy(outboundID: response.outboundID)
                )
            }
            return response.dnsMessage
        } catch {
            guard observation.decision.result.failureMode == .open else {
                report(
                    observation,
                    path: .decision,
                    errorCode: "NP_DNS_PROXY_RESOLVE_FAILED"
                )
                throw error
            }
            return try await resolveProxyFallback(
                request,
                question: question,
                observation: observation,
                network: network
            )
        }
    }

    private func resolveProxyFallback(
        _ original: Nonproxy_Provider_V1_ResolveDnsRequest,
        question: DNSQuestion,
        observation: ProviderDecisionObservation,
        network: MacNetworkEnvironmentSnapshot
    ) async throws -> Data {
        let interfaceIndex = network.preferredInterfaceIndex
        guard interfaceIndex > 0 else {
            report(
                observation,
                path: .decision,
                errorCode: "NP_DNS_PROXY_FAIL_OPEN_FAILED"
            )
            throw DNSProxyError.resolverUnavailable(
                "代理 DNS 失败且没有可用的物理网卡"
            )
        }
        var request = original
        request.requestedRoute = .direct
        request.requestedOutboundID = ""
        request.requestedOutboundGroupID = ""
        request.directInterfaceIndex = interfaceIndex
        do {
            let response = try await resolver.resolveDNS(request)
            try DNSResponseValidator.validate(
                response,
                request: request,
                question: question
            )
            if response.cacheHit {
                report(
                    observation,
                    path: .decision,
                    errorCode: "NP_DNS_PROXY_FAIL_OPEN_CACHE_HIT"
                )
            } else {
                report(
                    observation,
                    path: .direct(
                        interfaceName: "ifindex:\(interfaceIndex)",
                        failOpen: true
                    ),
                    errorCode: "NP_DNS_PROXY_FAIL_OPEN_DIRECT"
                )
            }
            return response.dnsMessage
        } catch {
            report(
                observation,
                path: .decision,
                errorCode: "NP_DNS_PROXY_FAIL_OPEN_FAILED"
            )
            throw error
        }
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

    private func report(
        _ observation: ProviderDecisionObservation,
        path: ProviderObservedPath,
        errorCode: String? = nil
    ) {
        guard let record = try? observation.record(
            path: path,
            errorCode: errorCode
        ) else {
            decisions.recordUnreportable()
            return
        }
        decisions.submit(record)
    }
}
