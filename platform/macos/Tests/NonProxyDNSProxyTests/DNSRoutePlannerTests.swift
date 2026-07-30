@testable import NonProxyDNSProxy
import NonProxyProviderContracts
import NonProxyProviderCore
import XCTest

final class DNSRoutePlannerTests: XCTestCase {
    func testMapsDirectProxyAndBlock() {
        XCTAssertEqual(
            DNSRoutePlanner.plan(decision: decision(action: .direct)),
            .direct
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .proxy, outboundID: "office")
            ),
            .proxy(outboundID: "office")
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(decision: decision(action: .block)),
            .refuse
        )
    }

    func testInvalidProxyOnlyFallsBackWhenExplicitlyOpen() {
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .proxy, failureMode: .closed)
            ),
            .refuse
        )
        XCTAssertEqual(
            DNSRoutePlanner.plan(
                decision: decision(action: .proxy, failureMode: .open)
            ),
            .direct
        )
    }

    private func decision(
        action: Nonproxy_Common_V1_RouteAction,
        outboundID: String = "",
        failureMode: Nonproxy_Common_V1_FailureMode = .closed
    ) -> PolicyDecision {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = action
        result.outboundID = outboundID
        result.failureMode = failureMode
        return PolicyDecision(
            result: result,
            matchedPolicyID: nil,
            snapshotVersion: 7,
            reasonCode: "test"
        )
    }
}
