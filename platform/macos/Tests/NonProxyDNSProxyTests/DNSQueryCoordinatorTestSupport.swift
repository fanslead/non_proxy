import Foundation
@testable import NonProxyDNSProxy
import NonProxyProviderContracts
import NonProxyProviderCore
import Synchronization

actor RecordingDNSResolver: ProviderDNSResolving {
    private(set) var lastRequest: Nonproxy_Provider_V1_ResolveDnsRequest?
    private(set) var requests: [Nonproxy_Provider_V1_ResolveDnsRequest] = []
    private(set) var routes: [Nonproxy_Provider_V1_DnsRouteKind] = []
    private let failProxy: Bool
    private let cacheHit: Bool
    private let selectedOutboundID: String?

    init(
        failProxy: Bool = false,
        cacheHit: Bool = false,
        selectedOutboundID: String? = nil
    ) {
        self.failProxy = failProxy
        self.cacheHit = cacheHit
        self.selectedOutboundID = selectedOutboundID
    }

    func resolveDNS(
        _ request: Nonproxy_Provider_V1_ResolveDnsRequest
    ) async throws -> Nonproxy_Provider_V1_ResolveDnsResponse {
        lastRequest = request
        requests.append(request)
        routes.append(request.requestedRoute)
        if failProxy, request.requestedRoute == .proxy {
            throw ProviderError.dnsResolution(
                code: "NP_TEST_PROXY_FAILED",
                message: "测试代理解析失败"
            )
        }
        let question = try DNSMessageParser.parseQuery(request.dnsMessage)
        var response = Nonproxy_Provider_V1_ResolveDnsResponse()
        response.dnsMessage = DNSResponseBuilder.refused(
            query: request.dnsMessage,
            question: question
        )
        response.route = request.requestedRoute
        response.outboundID = selectedOutboundID ?? request.requestedOutboundID
        response.cacheHit = cacheHit
        return response
    }
}

final class RecordingDecisionSubmitter:
    ProviderDecisionSubmitting,
    Sendable
{
    private let storage = Mutex<[Nonproxy_Provider_V1_DecisionRecord]>([])

    var records: [Nonproxy_Provider_V1_DecisionRecord] {
        storage.withLock { $0 }
    }

    func submit(
        _ decision: Nonproxy_Provider_V1_DecisionRecord
    ) -> Bool {
        storage.withLock { $0.append(decision) }
        return true
    }

    func recordUnreportable() {}
}
