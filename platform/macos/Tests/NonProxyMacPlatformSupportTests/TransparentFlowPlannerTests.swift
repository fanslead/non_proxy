import NonProxyMacPlatformSupport
import NonProxyProviderContracts
import NonProxyProviderCore
import XCTest

final class TransparentFlowPlannerTests: XCTestCase {
    func testDirectSelectsTheDedicatedDirectRelay() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .direct, failureMode: .closed),
                proxyTarget: nil,
                proxyRelayAvailable: false
            ),
            .direct
        )
    }

    func testUnavailableProxyUsesExplicitFailureMode() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .open),
                proxyTarget: .outbound(id: "proxy"),
                proxyRelayAvailable: false
            ),
            .direct
        )
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed),
                proxyTarget: .outbound(id: "proxy"),
                proxyRelayAvailable: false
            ),
            .reject(errorCode: "NP_PROXY_RELAY_UNAVAILABLE")
        )
    }

    func testAvailableProxyKeepsSelectedOutbound() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed),
                proxyTarget: .outbound(id: "proxy"),
                proxyRelayAvailable: true
            ),
            .proxy(target: .outbound(id: "proxy"))
        )
    }

    func testAvailableProxyKeepsTheImmutableGroupTarget() {
        let target = ProviderProxyTarget.group(
            id: "automatic",
            snapshotVersion: 1,
            memberIDs: ["primary", "backup"]
        )
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(
                    action: .proxy,
                    failureMode: .closed,
                    outboundGroupID: "automatic"
                ),
                proxyTarget: target,
                proxyRelayAvailable: true
            ),
            .proxy(target: target)
        )
    }

    func testMismatchedTargetUsesExplicitFailureMode() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed),
                proxyTarget: .outbound(id: "other"),
                proxyRelayAvailable: true
            ),
            .reject(errorCode: "NP_PROXY_TARGET_INVALID")
        )
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .proxy, failureMode: .open),
                proxyTarget: nil,
                proxyRelayAvailable: true
            ),
            .direct
        )
    }

    func testBlockIsNeverHandedBackAsDirect() {
        XCTAssertEqual(
            TransparentFlowPlanner.plan(
                decision: decision(action: .block, failureMode: .open),
                proxyTarget: nil,
                proxyRelayAvailable: false
            ),
            .reject(errorCode: "NP_POLICY_BLOCKED")
        )
    }

    private func decision(
        action: Nonproxy_Common_V1_RouteAction,
        failureMode: Nonproxy_Common_V1_FailureMode,
        outboundGroupID: String = ""
    ) -> PolicyDecision {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = action
        result.failureMode = failureMode
        result.outboundID = action == .proxy && outboundGroupID.isEmpty
            ? "proxy"
            : ""
        result.outboundGroupID = outboundGroupID
        return PolicyDecision(
            result: result,
            matchedPolicyID: "test",
            snapshotVersion: 1,
            reasonCode: "NP_TEST"
        )
    }
}
