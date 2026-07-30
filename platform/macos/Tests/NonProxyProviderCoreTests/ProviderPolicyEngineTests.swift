import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import XCTest

final class ProviderPolicyEngineTests: XCTestCase {
    func testAppTierWinsOverDestinationPriority() {
        let destination = policy(
            id: "destination",
            source: .site,
            priority: 10_000,
            matcher: domainMatcher("example.com", kind: .suffix),
            action: .direct
        )
        let application = policy(
            id: "application",
            source: .app,
            priority: 0,
            matcher: appMatcher("com.example.app"),
            action: .proxy
        )

        let decision = ProviderPolicyEngine.decide(
            snapshot: snapshot([application, destination]),
            context: context(
                appID: "com.example.app",
                domain: "api.example.com",
                ip: "203.0.113.10"
            )
        )

        XCTAssertEqual(decision.result.action, .proxy)
        XCTAssertEqual(decision.matchedPolicyID, "application")
        XCTAssertEqual(decision.reasonCode, "NP_POLICY_APP_MATCH")
    }

    func testExactDomainWinsAtEqualPriority() {
        let suffix = policy(
            id: "a-suffix",
            source: .site,
            priority: 10,
            matcher: domainMatcher("example.com", kind: .suffix),
            action: .proxy
        )
        let exact = policy(
            id: "z-exact",
            source: .site,
            priority: 10,
            matcher: domainMatcher("api.example.com", kind: .exact),
            action: .direct
        )

        let decision = ProviderPolicyEngine.decide(
            snapshot: snapshot([suffix, exact]),
            context: context(
                appID: "unknown-app",
                domain: "api.example.com",
                ip: nil
            )
        )

        XCTAssertEqual(decision.result.action, .direct)
        XCTAssertEqual(decision.matchedPolicyID, "z-exact")
    }

    func testLongestCidrWinsAtEqualPriority() {
        let broad = policy(
            id: "broad",
            source: .cidr,
            priority: 10,
            matcher: cidrMatcher("10.0.0.0", prefix: 8),
            action: .proxy
        )
        let narrow = policy(
            id: "narrow",
            source: .cidr,
            priority: 10,
            matcher: cidrMatcher("10.20.0.0", prefix: 16),
            action: .direct
        )

        let decision = ProviderPolicyEngine.decide(
            snapshot: snapshot([broad, narrow]),
            context: context(
                appID: "unknown-app",
                domain: nil,
                ip: "10.20.30.40"
            )
        )

        XCTAssertEqual(decision.result.action, .direct)
        XCTAssertEqual(decision.matchedPolicyID, "narrow")
    }

    func testRegistrableRuleMatchesAnySubdomainWithoutCallerPslLookup() {
        let registrable = policy(
            id: "registrable",
            source: .site,
            priority: 10,
            matcher: domainMatcher("example.co.uk", kind: .registrableDomain),
            action: .direct
        )

        let decision = ProviderPolicyEngine.decide(
            snapshot: snapshot([registrable]),
            context: context(
                appID: "unknown-app",
                domain: "cdn.api.example.co.uk",
                ip: nil
            )
        )

        XCTAssertEqual(decision.matchedPolicyID, "registrable")
    }

    private func policy(
        id: String,
        source: Nonproxy_Policy_V1_PolicySourceKind,
        priority: Int32,
        matcher: Nonproxy_Policy_V1_PolicyMatch,
        action: Nonproxy_Common_V1_RouteAction
    ) -> Nonproxy_Policy_V1_Policy {
        var decision = Nonproxy_Policy_V1_DecisionSpec()
        decision.action = action
        decision.failureMode = .closed
        if action == .proxy {
            decision.outboundID = "proxy"
        }

        var policy = Nonproxy_Policy_V1_Policy()
        policy.id = id
        policy.sourceKind = source
        policy.match = matcher
        policy.decision = decision
        policy.priority = priority
        policy.enabled = true
        return policy
    }

    private func appMatcher(
        _ stableID: String
    ) -> Nonproxy_Policy_V1_PolicyMatch {
        var app = Nonproxy_Policy_V1_AppMatcher()
        app.platform = .macos
        app.stableID = stableID
        var matcher = Nonproxy_Policy_V1_PolicyMatch()
        matcher.app = app
        return matcher
    }

    private func domainMatcher(
        _ pattern: String,
        kind: Nonproxy_Policy_V1_DomainMatchKind
    ) -> Nonproxy_Policy_V1_PolicyMatch {
        var domain = Nonproxy_Policy_V1_DomainMatcher()
        domain.kind = kind
        domain.asciiPattern = pattern
        var matcher = Nonproxy_Policy_V1_PolicyMatch()
        matcher.domain = domain
        return matcher
    }

    private func cidrMatcher(
        _ network: String,
        prefix: UInt32
    ) -> Nonproxy_Policy_V1_PolicyMatch {
        var cidr = Nonproxy_Policy_V1_CidrMatcher()
        cidr.network = network
        cidr.prefixLength = prefix
        var matcher = Nonproxy_Policy_V1_PolicyMatch()
        matcher.cidr = cidr
        return matcher
    }

    private func context(
        appID: String,
        domain: String?,
        ip: String?
    ) -> PolicyConnectionContext {
        PolicyConnectionContext(
            app: PolicyAppIdentity(stableID: appID),
            destination: PolicyDestination(
                normalizedDomain: domain,
                registrableDomain: domain?.hasSuffix(".example.com") == true
                    ? "example.com"
                    : domain,
                ipAddress: ip,
                transport: .tcp,
                port: 443
            )
        )
    }

    private func snapshot(
        _ policies: [Nonproxy_Policy_V1_Policy]
    ) -> VerifiedPolicySnapshot {
        var defaultDecision = Nonproxy_Policy_V1_DecisionSpec()
        defaultDecision.action = .proxy
        defaultDecision.outboundID = "proxy"
        defaultDecision.failureMode = .closed

        var payload = Nonproxy_Policy_V1_CompiledPolicyPayload()
        payload.formatVersion = 1
        payload.policies = policies
        payload.capabilities = Nonproxy_Policy_V1_CompileCapabilitySet()
        payload.defaultDecision = defaultDecision

        var metadata = Nonproxy_Policy_V1_PolicySnapshotMetadata()
        metadata.snapshotVersion = 7
        var wire = Nonproxy_Policy_V1_CompiledPolicySnapshot()
        wire.metadata = metadata
        return VerifiedPolicySnapshot(wireSnapshot: wire, payload: payload)
    }
}
