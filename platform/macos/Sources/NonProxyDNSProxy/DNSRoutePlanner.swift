import NonProxyProviderContracts
import NonProxyProviderCore

public enum DNSRoutePlan: Equatable, Sendable {
    case direct
    case proxy(outboundID: String)
    case refuse
}

public enum DNSRoutePlanner {
    public static func plan(decision: PolicyDecision) -> DNSRoutePlan {
        switch decision.result.action {
        case .direct:
            return .direct
        case .proxy where !decision.result.outboundID.isEmpty:
            return .proxy(outboundID: decision.result.outboundID)
        case .block:
            return .refuse
        default:
            return decision.result.failureMode == .open ? .direct : .refuse
        }
    }
}
