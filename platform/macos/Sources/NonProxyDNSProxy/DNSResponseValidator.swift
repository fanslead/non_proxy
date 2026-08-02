import NonProxyProviderContracts
import NonProxyProviderCore

enum DNSResponseValidator {
    static func validate(
        _ response: Nonproxy_Provider_V1_ResolveDnsResponse,
        request: Nonproxy_Provider_V1_ResolveDnsRequest,
        question: DNSQuestion,
        proxyTarget: ProviderProxyTarget? = nil
    ) throws {
        guard response.route == request.requestedRoute else {
            throw DNSProxyError.responseInvalid("DNS 响应路由标签不匹配")
        }
        if request.requestedRoute == .proxy {
            try validateProxyLabels(
                response: response,
                request: request,
                proxyTarget: proxyTarget
            )
        } else if !response.outboundID.isEmpty
            || !request.requestedOutboundID.isEmpty
            || !request.requestedOutboundGroupID.isEmpty {
            throw DNSProxyError.responseInvalid("非代理 DNS 携带了代理出口标签")
        }
        try DNSMessageParser.validateResponse(
            response.dnsMessage,
            for: question
        )
    }

    private static func validateProxyLabels(
        response: Nonproxy_Provider_V1_ResolveDnsResponse,
        request: Nonproxy_Provider_V1_ResolveDnsRequest,
        proxyTarget: ProviderProxyTarget?
    ) throws {
        guard let proxyTarget,
              proxyTarget.accepts(selectedOutboundID: response.outboundID)
        else {
            throw DNSProxyError.responseInvalid("DNS 响应代理出口标签不匹配")
        }
        switch proxyTarget {
        case .outbound(let id):
            guard request.requestedOutboundID == id,
                  request.requestedOutboundGroupID.isEmpty
            else {
                throw DNSProxyError.responseInvalid("DNS 单出口请求标签不匹配")
            }
        case .group(let id, let snapshotVersion, _):
            guard request.requestedOutboundID.isEmpty,
                  request.requestedOutboundGroupID == id,
                  request.snapshotVersion == snapshotVersion
            else {
                throw DNSProxyError.responseInvalid("DNS 出口组请求标签不匹配")
            }
        }
    }
}
