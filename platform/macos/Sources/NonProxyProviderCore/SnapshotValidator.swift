import Foundation
import NonProxyProviderContracts
import SwiftProtobuf

public enum SnapshotValidator {
    public static let schemaVersion: UInt32 = 1
    public static let payloadFormat = "nonproxy.compiled-policy.v1"
    public static let payloadVersion: UInt32 = 1
    public static let maximumPayloadBytes = 16 * 1024 * 1024

    public static func validate(
        _ snapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot
    ) throws -> VerifiedPolicySnapshot {
        try validateEnvelope(snapshot)
        let payload: Nonproxy_Policy_V1_CompiledPolicyPayload
        do {
            payload = try Nonproxy_Policy_V1_CompiledPolicyPayload(
                serializedBytes: snapshot.payload
            )
        } catch {
            throw ProviderError.invalidSnapshot("策略快照载荷无法解码")
        }
        try validatePayload(payload, against: snapshot)

        let actualHash = try CanonicalSnapshotHasher.hash(
            schemaVersion: snapshot.metadata.schemaVersion,
            payload: payload
        )
        guard actualHash == snapshot.metadata.contentHash else {
            throw ProviderError.invalidSnapshot("策略快照内容哈希不匹配")
        }
        return VerifiedPolicySnapshot(wireSnapshot: snapshot, payload: payload)
    }

    private static func validateEnvelope(
        _ snapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot
    ) throws {
        guard snapshot.hasMetadata,
              snapshot.metadata.schemaVersion == schemaVersion,
              snapshot.metadata.snapshotVersion > 0,
              snapshot.metadata.contentHash.count == SHA256.byteCount,
              snapshot.payloadFormat == payloadFormat,
              !snapshot.payload.isEmpty,
              snapshot.payload.count <= maximumPayloadBytes,
              snapshot.hasDefaultDecision
        else {
            throw ProviderError.invalidSnapshot("策略快照信封不完整或版本不受支持")
        }
        guard snapshot.metadata.state == .pendingAck
                || snapshot.metadata.state == .active
        else {
            throw ProviderError.invalidSnapshot("策略快照不是可加载状态")
        }
    }

    private static func validatePayload(
        _ payload: Nonproxy_Policy_V1_CompiledPolicyPayload,
        against snapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot
    ) throws {
        guard payload.formatVersion == payloadVersion,
              payload.hasCapabilities,
              payload.hasDefaultDecision,
              payload.policies.count == Int(snapshot.metadata.policyCount),
              sameDecision(payload.defaultDecision, snapshot.defaultDecision)
        else {
            throw ProviderError.invalidSnapshot("策略快照载荷元数据不一致")
        }

        var policyIDs = Set<String>()
        var previousID: String?
        for policy in payload.policies {
            guard policy.enabled,
                  policy.hasMatch,
                  policy.hasDecision,
                  policyIDs.insert(policy.id).inserted
            else {
                throw ProviderError.invalidSnapshot("策略快照包含无效或重复策略")
            }
            if let previousID, previousID >= policy.id {
                throw ProviderError.invalidSnapshot("策略快照未按稳定标识排序")
            }
            previousID = policy.id
            try SnapshotContentValidator.validatePolicy(policy)
        }
        try validateCapabilities(
            payload.capabilities,
            policies: payload.policies,
            defaultDecision: payload.defaultDecision
        )
        try SnapshotContentValidator.validateDecision(
            payload.defaultDecision,
            availableOutbounds: Set(payload.capabilities.outbounds.map(\.outboundID))
        )
    }

    private static func validateCapabilities(
        _ capabilities: Nonproxy_Policy_V1_CompileCapabilitySet,
        policies: [Nonproxy_Policy_V1_Policy],
        defaultDecision: Nonproxy_Policy_V1_DecisionSpec
    ) throws {
        guard validOrderedUnique(capabilities.transports, allowed: [.tcp, .udp]),
              validOrderedUnique(capabilities.ipFamilies, allowed: [.ipv4, .ipv6]),
              !capabilities.transports.isEmpty,
              !capabilities.ipFamilies.isEmpty
        else {
            throw ProviderError.invalidSnapshot("策略快照目标能力无效")
        }

        var outboundIDs = Set<String>()
        var previousOutboundID: String?
        for outbound in capabilities.outbounds {
            guard SnapshotContentValidator.isIdentifier(outbound.outboundID),
                  outboundIDs.insert(outbound.outboundID).inserted,
                  previousOutboundID.map({ $0 < outbound.outboundID }) ?? true,
                  validOrderedUnique(outbound.transports, allowed: [.tcp, .udp]),
                  validOrderedUnique(outbound.ipFamilies, allowed: [.ipv4, .ipv6])
            else {
                throw ProviderError.invalidSnapshot("策略快照出口能力无效")
            }
            previousOutboundID = outbound.outboundID
        }
        try SnapshotContentValidator.validateCapabilities(
            capabilities,
            policies: policies,
            defaultDecision: defaultDecision
        )
    }

    private static func validOrderedUnique<T: Hashable & RawRepresentable>(
        _ values: [T],
        allowed: Set<T>
    ) -> Bool where T.RawValue: Comparable {
        Set(values).count == values.count
            && values.allSatisfy(allowed.contains)
            && zip(values, values.dropFirst()).allSatisfy {
                $0.rawValue < $1.rawValue
            }
    }

    private static func sameDecision(
        _ left: Nonproxy_Policy_V1_DecisionSpec,
        _ right: Nonproxy_Policy_V1_DecisionSpec
    ) -> Bool {
        left.action == right.action
            && left.outboundID == right.outboundID
            && left.failureMode == right.failureMode
    }

    private enum SHA256 {
        static let byteCount = 32
    }
}
