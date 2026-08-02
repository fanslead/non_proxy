import NonProxyProviderContracts
import NonProxyProviderCore

public enum DNSRoutePlan: Equatable, Sendable {
    case direct
    case proxy(target: ProviderProxyTarget)
    case refuse
}

public enum DNSRoutePlanner {
    public static func plan(
        decision: PolicyDecision,
        proxyTarget: ProviderProxyTarget?
    ) -> DNSRoutePlan {
        switch decision.result.action {
        case .direct:
            return .direct
        case .proxy:
            guard let proxyTarget,
                  proxyTarget.matches(decision: decision)
            else {
                return decision.result.failureMode == .open ? .direct : .refuse
            }
            return .proxy(target: proxyTarget)
        case .block:
            return .refuse
        default:
            return .refuse
        }
    }
}
