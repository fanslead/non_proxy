import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import XCTest

final class ProviderDecisionObservationTests: XCTestCase {
    func testEncodesFailOpenPathWithoutClaimingProxyOutbound() throws {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = .proxy
        result.outboundID = "office"
        result.failureMode = .open
        let observation = ProviderDecisionObservation(
            flowID: "flow-1",
            context: context(),
            decision: PolicyDecision(
                result: result,
                matchedPolicyID: "policy-1",
                matchedRuleID: "rule-1",
                snapshotVersion: 7,
                reasonCode: "NP_POLICY_APP_MATCH"
            ),
            observedAt: Date(timeIntervalSince1970: 1_800_000_000.125),
            decisionLatencyNanoseconds: 125_000
        )

        let record = try observation.record(
            path: .direct(interfaceName: "en0", failOpen: true),
            errorCode: "NP_PROXY_FAIL_OPEN_DIRECT"
        )

        XCTAssertEqual(record.context.flowID, "flow-1")
        XCTAssertEqual(record.context.app.platform, .macos)
        XCTAssertEqual(record.context.destination.normalizedDomain, "api.example.com")
        XCTAssertEqual(record.decision.matchedRuleID, "rule-1")
        XCTAssertEqual(record.evidence.level, .path)
        XCTAssertEqual(record.evidence.interfaceName, "en0")
        XCTAssertTrue(record.evidence.failOpenDirect)
        XCTAssertTrue(record.evidence.outboundID.isEmpty)
        XCTAssertEqual(record.error.code, "NP_PROXY_FAIL_OPEN_DIRECT")
        XCTAssertEqual(record.decisionLatency.nanos, 125_000)
    }

    func testRejectsFailOpenEvidenceForClosedProxyPolicy() {
        var result = Nonproxy_Policy_V1_DecisionSpec()
        result.action = .proxy
        result.outboundID = "office"
        result.failureMode = .closed
        let observation = ProviderDecisionObservation(
            flowID: "flow-2",
            context: context(),
            decision: PolicyDecision(
                result: result,
                matchedPolicyID: nil,
                snapshotVersion: 7,
                reasonCode: "NP_POLICY_DEFAULT"
            ),
            observedAt: Date(),
            decisionLatencyNanoseconds: 1
        )

        XCTAssertThrowsError(try observation.record(
            path: .direct(interfaceName: "en0", failOpen: true),
            errorCode: "NP_PROXY_FAIL_OPEN_DIRECT"
        ))
    }

    private func context() -> PolicyConnectionContext {
        PolicyConnectionContext(
            app: PolicyAppIdentity(
                stableID: "com.example.browser",
                signerID: "TEAM"
            ),
            destination: PolicyDestination(
                normalizedDomain: "api.example.com",
                registrableDomain: "example.com",
                ipAddress: nil,
                transport: .tcp,
                port: 443
            )
        )
    }
}
