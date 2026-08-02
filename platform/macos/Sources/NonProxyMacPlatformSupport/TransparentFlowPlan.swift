import NonProxyProviderContracts
import NonProxyProviderCore

public enum TransparentFlowPlan: Sendable, Equatable {
    case direct
    case proxy(target: ProviderProxyTarget)
    case reject(errorCode: String)
}

public enum TransparentFlowPlanner {
    public static func plan(
        decision: PolicyDecision,
        proxyTarget: ProviderProxyTarget?,
        proxyRelayAvailable: Bool
    ) -> TransparentFlowPlan {
        switch decision.result.action {
        case .direct:
            return .direct
        case .block:
            return .reject(errorCode: "NP_POLICY_BLOCKED")
        case .proxy where proxyRelayAvailable:
            guard let proxyTarget,
                  proxyTarget.matches(decision: decision)
            else {
                return decision.result.failureMode == .open
                    ? .direct
                    : .reject(errorCode: "NP_PROXY_TARGET_INVALID")
            }
            return .proxy(target: proxyTarget)
        case .proxy where decision.result.failureMode == .open:
            return .direct
        case .proxy:
            return .reject(errorCode: "NP_PROXY_RELAY_UNAVAILABLE")
        default:
            return .reject(errorCode: "NP_POLICY_DECISION_INVALID")
        }
    }
}
