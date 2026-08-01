import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import SwiftProtobuf
import XCTest

final class ProviderPolicyRuntimeTests: XCTestCase {
    func testInstallsAndUsesLatestImmutableSnapshot() throws {
        let runtime = ProviderPolicyRuntime()
        let first = try verifiedSnapshot(version: 1, action: .block)
        let second = try verifiedSnapshot(version: 2, action: .direct)

        XCTAssertTrue(try runtime.install(first))
        XCTAssertTrue(try runtime.install(second))
        XCTAssertEqual(runtime.activeSnapshotVersion, 2)
        XCTAssertEqual(try runtime.decide(context: context()).result.action, .direct)
    }

    func testRejectsSnapshotDowngrade() throws {
        let runtime = ProviderPolicyRuntime()
        try runtime.install(verifiedSnapshot(version: 2, action: .direct))

        XCTAssertThrowsError(
            try runtime.install(verifiedSnapshot(version: 1, action: .direct))
        ) { error in
            XCTAssertEqual(
                (error as? ProviderError)?.code,
                "NP_PROVIDER_SNAPSHOT_INVALID"
            )
        }
        XCTAssertEqual(runtime.activeSnapshotVersion, 2)
    }

    func testRejectsSameVersionWithDifferentHash() throws {
        let runtime = ProviderPolicyRuntime()
        try runtime.install(verifiedSnapshot(version: 4, action: .direct))

        XCTAssertThrowsError(
            try runtime.install(verifiedSnapshot(version: 4, action: .block))
        )
    }

    func testResolvesMostSpecificNetworkFingerprintInDecisionSnapshot() throws {
        let ssidHash = String(repeating: "a", count: 64)
        let runtime = ProviderPolicyRuntime()
        try runtime.install(
            networkSnapshot(ssidHash: ssidHash)
        )

        let evaluation = try runtime.evaluate(
            context: context(),
            networkFingerprints: [
                try PolicyNetworkFingerprint(
                    kind: .interfaceClass,
                    value: "wifi"
                ),
                try PolicyNetworkFingerprint(
                    kind: .wifiSsidSha256,
                    value: ssidHash
                ),
            ]
        )

        XCTAssertEqual(evaluation.context.networkProfileID, "office")
        XCTAssertEqual(evaluation.decision?.result.action, .block)
        XCTAssertEqual(evaluation.decision?.matchedPolicyID, "office-direct")
        XCTAssertEqual(evaluation.decision?.reasonCode, "NP_POLICY_NETWORK_MATCH")

        let fallback = try runtime.evaluate(
            context: context(),
            networkFingerprints: [
                try PolicyNetworkFingerprint(
                    kind: .interfaceClass,
                    value: "wifi"
                ),
            ]
        )
        XCTAssertEqual(fallback.context.networkProfileID, "any-wifi")
        XCTAssertEqual(fallback.decision?.result.action, .direct)
        XCTAssertNil(fallback.decision?.matchedPolicyID)
    }

    func testPauseBypassesOnlyUntilItsAbsoluteExpiry() throws {
        let runtime = ProviderPolicyRuntime()
        try runtime.install(
            try runtimeOverrideSnapshot(mode: .paused, expiresAtSeconds: 2)
        )

        let active = try runtime.evaluate(
            context: context(),
            at: Date(timeIntervalSince1970: 1.999)
        )
        guard case .bypass(let version, let reason) = active.disposition else {
            return XCTFail("暂停覆盖应返回系统旁路")
        }
        XCTAssertEqual(version, 10)
        XCTAssertEqual(reason, "NP_RUNTIME_OVERRIDE_PAUSED")

        let expired = try runtime.evaluate(
            context: context(),
            at: Date(timeIntervalSince1970: 2)
        )
        XCTAssertEqual(expired.decision?.result.action, .block)
        XCTAssertEqual(expired.decision?.reasonCode, "NP_POLICY_DEFAULT")
    }

    func testDirectAndProxyOverridesProduceFailClosedDecisions() throws {
        let direct = ProviderPolicyRuntime()
        try direct.install(
            try runtimeOverrideSnapshot(mode: .direct, expiresAtSeconds: 2)
        )
        let directDecision = try direct.evaluate(
            context: context(),
            at: Date(timeIntervalSince1970: 1.5)
        ).decision
        XCTAssertEqual(directDecision?.result.action, .direct)
        XCTAssertEqual(directDecision?.reasonCode, "NP_RUNTIME_OVERRIDE_DIRECT")

        let proxy = ProviderPolicyRuntime()
        try proxy.install(
            try runtimeOverrideSnapshot(
                mode: .proxy,
                outboundID: "proxy",
                expiresAtSeconds: 2
            )
        )
        let proxyDecision = try proxy.evaluate(
            context: context(),
            at: Date(timeIntervalSince1970: 1.5)
        ).decision
        XCTAssertEqual(proxyDecision?.result.action, .proxy)
        XCTAssertEqual(proxyDecision?.result.outboundID, "proxy")
        XCTAssertEqual(proxyDecision?.result.failureMode, .closed)
    }

    func testSystemRuleStillWinsDuringPause() throws {
        var policy = Nonproxy_Policy_V1_Policy()
        policy.id = "system-global"
        policy.displayName = "系统保护"
        policy.sourceKind = .system
        policy.match = Nonproxy_Policy_V1_PolicyMatch()
        var blocked = SnapshotFixtures.directDecision()
        blocked.action = .block
        policy.decision = blocked
        policy.enabled = true
        policy.origin = .system
        policy.revision = 1
        let runtime = ProviderPolicyRuntime()
        try runtime.install(
            try runtimeOverrideSnapshot(
                mode: .paused,
                expiresAtSeconds: 2,
                policies: [policy]
            )
        )

        let evaluation = try runtime.evaluate(
            context: context(),
            at: Date(timeIntervalSince1970: 1.5)
        )
        XCTAssertEqual(evaluation.decision?.result.action, .block)
        XCTAssertEqual(evaluation.decision?.reasonCode, "NP_POLICY_SYSTEM_MATCH")
    }

    private func verifiedSnapshot(
        version: UInt64,
        action: Nonproxy_Common_V1_RouteAction
    ) throws -> VerifiedPolicySnapshot {
        var decision = SnapshotFixtures.directDecision()
        decision.action = action
        let payload = SnapshotFixtures.payload(defaultDecision: decision)
        return try SnapshotValidator.validate(
            SnapshotFixtures.snapshot(payload: payload, version: version)
        )
    }

    private func context() -> PolicyConnectionContext {
        PolicyConnectionContext(
            app: .unknown,
            destination: PolicyDestination(
                normalizedDomain: "example.com",
                registrableDomain: "example.com",
                ipAddress: "203.0.113.10",
                transport: .tcp,
                port: 443
            )
        )
    }

    private func runtimeOverrideSnapshot(
        mode: Nonproxy_Policy_V1_RuntimeOverrideMode,
        outboundID: String = "",
        expiresAtSeconds: Int64,
        policies: [Nonproxy_Policy_V1_Policy] = []
    ) throws -> VerifiedPolicySnapshot {
        var capabilities = SnapshotFixtures.fullCapabilities()
        if mode == .proxy {
            var outbound = Nonproxy_Policy_V1_OutboundCapabilitySpec()
            outbound.outboundID = outboundID
            outbound.transports = [.tcp, .udp]
            outbound.ipFamilies = [.ipv4, .ipv6]
            capabilities.outbounds = [outbound]
        }
        var runtimeOverride = Nonproxy_Policy_V1_RuntimeRoutingOverride()
        runtimeOverride.mode = mode
        runtimeOverride.outboundID = outboundID
        var expiresAt = Google_Protobuf_Timestamp()
        expiresAt.seconds = expiresAtSeconds
        runtimeOverride.expiresAt = expiresAt
        var blocked = SnapshotFixtures.directDecision()
        blocked.action = .block
        var payload = SnapshotFixtures.payload(
            policies: policies,
            capabilities: capabilities,
            defaultDecision: blocked
        )
        payload.runtimeOverride = runtimeOverride
        return try SnapshotValidator.validate(
            SnapshotFixtures.snapshot(payload: payload, version: 10)
        )
    }

    private func networkSnapshot(
        ssidHash: String
    ) throws -> VerifiedPolicySnapshot {
        var interfaceProfile = Nonproxy_Policy_V1_NetworkProfileBinding()
        interfaceProfile.id = "any-wifi"
        interfaceProfile.fingerprintKind = .interfaceClass
        interfaceProfile.fingerprintValue = "wifi"

        var officeProfile = Nonproxy_Policy_V1_NetworkProfileBinding()
        officeProfile.id = "office"
        officeProfile.fingerprintKind = .wifiSsidSha256
        officeProfile.fingerprintValue = ssidHash

        var network = Nonproxy_Policy_V1_NetworkMatcher()
        network.profileID = "office"
        var matcher = Nonproxy_Policy_V1_PolicyMatch()
        matcher.network = network
        var decision = SnapshotFixtures.directDecision()
        decision.action = .block
        var policy = Nonproxy_Policy_V1_Policy()
        policy.id = "office-direct"
        policy.displayName = "办公网络直连"
        policy.sourceKind = .network
        policy.match = matcher
        policy.decision = decision
        policy.enabled = true
        policy.origin = .user
        policy.revision = 1

        var payload = SnapshotFixtures.payload(policies: [policy])
        payload.networkProfiles = [interfaceProfile, officeProfile]
        return try SnapshotValidator.validate(
            SnapshotFixtures.snapshot(payload: payload, version: 9)
        )
    }
}
