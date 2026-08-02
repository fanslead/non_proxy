@testable import NonProxyDNSProxy
import NonProxyProviderContracts
import NonProxyProviderCore
import XCTest

final class DNSRoutePlannerTests: XCTestCase {
    func testMapsDirectProxyAndBlock() {
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .direct),
                proxyTarget: nil
            ),
            .direct
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .proxy, outboundID: "office"),
                proxyTarget: .outbound(id: "office")
            ),
            .proxy(target: .outbound(id: "office"))
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .block),
                proxyTarget: nil
            ),
            .refuse
        )
    }

    func testInvalidProxyOnlyFallsBackWhenExplicitlyOpen() {
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed),
                proxyTarget: nil
            ),
            .refuse
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .proxy, failureMode: .open),
                proxyTarget: nil
            ),
            .direct
        )
    }

    func testMapsAnImmutableOutboundGroupTarget() {
        let target = ProviderProxyTarget.group(
            id: "automatic",
            snapshotVersion: 7,
            memberIDs: ["primary", "backup"]
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(
                    action: .proxy,
                    outboundGroupID: "automatic"
                ),
                proxyTarget: target
            ),
            .proxy(target: target)
        )
    }

    private func decision(
        action: Nonproxy_Common_V1_RouteAction,
        outboundID: String = "",
        outboundGroupID: String = "",
        failureMode: Nonproxy_Common_V1_FailureMode = .closed
    ) -> PolicyDecision {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = action
        result.outboundID = outboundID
        result.outboundGroupID = outboundGroupID
        result.failureMode = failureMode
        return PolicyDecision(
            result: result,
            matchedPolicyID: nil,
            snapshotVersion: 7,
            reasonCode: "test"
        )
    }
}
