import NonProxyProviderContracts
import NonProxyProviderCore

public enum TransparentFlowPlan: Sendable, Equatable {
    case direct
    case proxy(outboundID: String)
    case reject(errorCode: String)
}

public enum TransparentFlowPlanner {
    public static func plan(
        decision: PolicyDecision,
        proxyRelayAvailable: Bool
    ) -> TransparentFlowPlan {
        switch decision.result.action {
        case .direct:
            return .direct
        case .block:
            return .reject(errorCode: "NP_POLICY_BLOCKED")
        case .proxy where proxyRelayAvailable:
            return .proxy(outboundID: decision.result.outboundID)
        case .proxy where decision.result.failureMode == .open:
            return .direct
        case .proxy:
            return .reject(errorCode: "NP_PROXY_RELAY_UNAVAILABLE")
        default:
            return .reject(errorCode: "NP_POLICY_DECISION_INVALID")
        }
    }
}
