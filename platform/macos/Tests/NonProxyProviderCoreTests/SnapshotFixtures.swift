import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore

enum SnapshotFixtures {
    static func directDecision() -> Nonproxy_Policy_V1_DecisionSpec {
        var decision = Nonproxy_Policy_V1_DecisionSpec()
        decision.action = .direct
        decision.failureMode = .closed
        return decision
    }

    static func fullCapabilities()
        -> Nonproxy_Policy_V1_CompileCapabilitySet
    {
        var capabilities = Nonproxy_Policy_V1_CompileCapabilitySet()
        capabilities.appMatch = true
        capabilities.domainMatch = true
        capabilities.cidrMatch = true
        capabilities.transports = [.tcp, .udp]
        capabilities.ipFamilies = [.ipv4, .ipv6]
        return capabilities
    }

    static func payload(
        policies: [Nonproxy_Policy_V1_Policy] = [],
        capabilities: Nonproxy_Policy_V1_CompileCapabilitySet? = nil,
        defaultDecision: Nonproxy_Policy_V1_DecisionSpec? = nil
    ) -> Nonproxy_Policy_V1_CompiledPolicyPayload {
        var payload = Nonproxy_Policy_V1_CompiledPolicyPayload()
        payload.formatVersion = SnapshotValidator.payloadVersion
        payload.policies = policies
        payload.capabilities = capabilities ?? fullCapabilities()
        payload.defaultDecision = defaultDecision ?? directDecision()
        return payload
    }

    static func snapshot(
        payload: Nonproxy_Policy_V1_CompiledPolicyPayload? = nil,
        version: UInt64 = 1,
        state: Nonproxy_Policy_V1_SnapshotState = .pendingAck
    ) throws -> Nonproxy_Policy_V1_CompiledPolicySnapshot {
        let payload = payload ?? self.payload()
        var metadata = Nonproxy_Policy_V1_PolicySnapshotMetadata()
        metadata.schemaVersion = 1
        metadata.snapshotVersion = version
        metadata.contentHash = try CanonicalSnapshotHasher.hash(
            schemaVersion: 1,
            payload: payload
        )
        metadata.state = state
        metadata.policyCount = UInt32(payload.policies.count)

        var snapshot = Nonproxy_Policy_V1_CompiledPolicySnapshot()
        snapshot.metadata = metadata
        snapshot.payloadFormat = SnapshotValidator.payloadFormat
        snapshot.payload = try payload.serializedData()
        snapshot.defaultDecision = payload.defaultDecision
        return snapshot
    }

    static func sitePolicy(
        id: String = "site-example",
        pattern: String = "example.com",
        action: Nonproxy_Common_V1_RouteAction = .direct
    ) -> Nonproxy_Policy_V1_Policy {
        var domain = Nonproxy_Policy_V1_DomainMatcher()
        domain.kind = .suffix
        domain.asciiPattern = pattern
        var matcher = Nonproxy_Policy_V1_PolicyMatch()
        matcher.domain = domain

        var decision = directDecision()
        decision.action = action
        if action == .proxy {
            decision.outboundID = "proxy"
        }

        var policy = Nonproxy_Policy_V1_Policy()
        policy.id = id
        policy.displayName = "示例站点"
        policy.sourceKind = .site
        policy.match = matcher
        policy.decision = decision
        policy.enabled = true
        policy.origin = .user
        policy.revision = 1
        return policy
    }
}
