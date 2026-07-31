import NonProxyProviderCore

enum ProxySetupRecoveryPlan: Equatable {
    case directFallback
    case reject(errorCode: String)
}

enum ProxySetupRecoveryPlanner {
    static func plan(
        decision: PolicyDecision,
        errorCode: String
    ) -> ProxySetupRecoveryPlan {
        if decision.result.action == .proxy,
           decision.result.failureMode == .open
        {
            return .directFallback
        }
        return .reject(errorCode: errorCode)
    }
}
