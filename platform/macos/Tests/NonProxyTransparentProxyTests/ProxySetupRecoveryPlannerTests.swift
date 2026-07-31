import NonProxyProviderContracts
@testable import NonProxyProviderCore
@testable import NonProxyTransparentProxy
import XCTest

final class ProxySetupRecoveryPlannerTests: XCTestCase {
    func testOpenProxyFallsBackToDirect() {
        let decision = policyDecision(
            action: .proxy,
            failureMode: .open
        )

        XCTAssertEqual(
            ProxySetupRecoveryPlanner.plan(
                decision: decision,
                errorCode: "NP_PROXY_CONNECT_FAILED"
            ),
            .directFallback
        )
    }

    func testClosedProxyPreservesTheSetupFailure() {
        let decision = policyDecision(
            action: .proxy,
            failureMode: .closed
        )

        XCTAssertEqual(
            ProxySetupRecoveryPlanner.plan(
                decision: decision,
                errorCode: "NP_PROXY_CONNECT_FAILED"
            ),
            .reject(errorCode: "NP_PROXY_CONNECT_FAILED")
        )
    }

    func testNonProxyDecisionNeverUsesProxyFallback() {
        let decision = policyDecision(
            action: .direct,
            failureMode: .open
        )

        XCTAssertEqual(
            ProxySetupRecoveryPlanner.plan(
                decision: decision,
                errorCode: "NP_PROXY_CONNECT_FAILED"
            ),
            .reject(errorCode: "NP_PROXY_CONNECT_FAILED")
        )
    }

    private func policyDecision(
        action: Nonproxy_Common_V1_RouteAction,
        failureMode: Nonproxy_Common_V1_FailureMode
    ) -> PolicyDecision {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = action
        result.failureMode = failureMode
        if action == .proxy {
            result.outboundID = "corp-proxy"
        }
        return PolicyDecision(
            result: result,
            matchedPolicyID: "policy-1",
            matchedRuleID: "policy-1",
            snapshotVersion: 1,
            reasonCode: "POLICY_MATCH"
        )
    }
}
