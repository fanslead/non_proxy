import NonProxyMacPlatformSupport
import NonProxyProviderContracts
import NonProxyProviderCore
import XCTest

final class TransparentFlowPlannerTests: XCTestCase {
    func testDirectSelectsTheDedicatedDirectRelay() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .direct, failureMode: .closed),
                proxyRelayAvailable: false
            ),
            .direct
        )
    }

    func testUnavailableProxyUsesExplicitFailureMode() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .open),
                proxyRelayAvailable: false
            ),
            .direct
        )
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed),
                proxyRelayAvailable: false
            ),
            .reject(errorCode: "NP_PROXY_RELAY_UNAVAILABLE")
        )
    }

    func testAvailableProxyKeepsSelectedOutbound() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed),
                proxyRelayAvailable: true
            ),
            .proxy(outboundID: "proxy")
        )
    }

    func testBlockIsNeverHandedBackAsDirect() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .block, failureMode: .open),
                proxyRelayAvailable: false
            ),
            .reject(errorCode: "NP_POLICY_BLOCKED")
        )
    }

    private func decision(
        action: Nonproxy_Common_V1_RouteAction,
        failureMode: Nonproxy_Common_V1_FailureMode
    ) -> PolicyDecision {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = action
        result.failureMode = failureMode
        result.outboundID = action == .proxy ? "proxy" : ""
        return PolicyDecision(
            result: result,
            matchedPolicyID: "test",
            snapshotVersion: 1,
            reasonCode: "NP_TEST"
        )
    }
}
