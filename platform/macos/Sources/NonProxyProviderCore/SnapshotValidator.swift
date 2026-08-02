import Foundation
import NonProxyProviderContracts
import SwiftProtobuf

public enum SnapshotValidator {
    public static let schemaVersion: UInt32 = 1
    public static let payloadFormat = "nonproxy.compiled-policy.v1"
    public static let legacyPayloadVersion: UInt32 = 1
    public static let networkProfilePayloadVersion: UInt32 = 2
    public static let runtimeOverridePayloadVersion: UInt32 = 3
    public static let payloadVersion: UInt32 = 4
    public static let maximumPayloadBytes = 16 * 1024 * 1024
    private static let maximumRuntimeOverrideMilliseconds: UInt64 = 60 * 60 * 1_000

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
        guard payload.formatVersion == legacyPayloadVersion
                || payload.formatVersion == networkProfilePayloadVersion
                || payload.formatVersion == runtimeOverridePayloadVersion
                || payload.formatVersion == payloadVersion,
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
        try validateNetworkProfiles(payload)
        try validateRuntimeOverride(payload, against: snapshot)
        try validateOutboundGroups(payload)
        try validateCapabilities(
            payload.capabilities,
            policies: payload.policies,
            defaultDecision: payload.defaultDecision
        )
        try SnapshotContentValidator.validateDecision(
            payload.defaultDecision,
            availableOutbounds: Set(payload.capabilities.outbounds.map(\.outboundID)),
            availableGroups: Set(payload.capabilities.outboundGroups.map(\.outboundGroupID))
        )
    }

    private static func validateNetworkProfiles(
        _ payload: Nonproxy_Policy_V1_CompiledPolicyPayload
    ) throws {
        if payload.formatVersion < networkProfilePayloadVersion {
            guard payload.networkProfiles.isEmpty else {
                throw ProviderError.invalidSnapshot("旧版策略快照包含网络配置档目录")
            }
            return
        }
        var profileIDs = Set<String>()
        var fingerprints = Set<String>()
        var previousID: String?
        for profile in payload.networkProfiles {
            let fingerprintKey = "\(profile.fingerprintKind.rawValue):\(profile.fingerprintValue)"
            guard profileIDs.insert(profile.id).inserted,
                  fingerprints.insert(fingerprintKey).inserted,
                  previousID.map({ $0 < profile.id }) ?? true
            else {
                throw ProviderError.invalidSnapshot("网络配置档目录重复或未排序")
            }
            previousID = profile.id
            try SnapshotContentValidator.validateNetworkProfile(profile)
        }
        for policy in payload.policies where policy.match.hasNetwork {
            guard profileIDs.contains(policy.match.network.profileID) else {
                throw ProviderError.invalidSnapshot("网络规则引用了未知配置档")
            }
        }
    }

    private static func validateRuntimeOverride(
        _ payload: Nonproxy_Policy_V1_CompiledPolicyPayload,
        against snapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot
    ) throws {
        if payload.formatVersion < runtimeOverridePayloadVersion {
            guard !payload.hasRuntimeOverride else {
                throw ProviderError.invalidSnapshot("旧版策略快照包含运行态覆盖")
            }
            return
        }
        guard payload.hasRuntimeOverride else {
            return
        }
        let runtimeOverride = payload.runtimeOverride
        guard runtimeOverride.hasExpiresAt,
              snapshot.metadata.hasCreatedAt
        else {
            throw ProviderError.invalidSnapshot("运行态覆盖缺少时间边界")
        }
        let expiresAt = try unixMilliseconds(runtimeOverride.expiresAt)
        let createdAt = try unixMilliseconds(snapshot.metadata.createdAt)
        guard expiresAt > createdAt,
              expiresAt - createdAt <= maximumRuntimeOverrideMilliseconds
        else {
            throw ProviderError.invalidSnapshot("运行态覆盖到期时间超出允许范围")
        }
        try SnapshotContentValidator.validateRuntimeOverride(
            runtimeOverride,
            capabilities: payload.capabilities
        )
    }

    private static func validateOutboundGroups(
        _ payload: Nonproxy_Policy_V1_CompiledPolicyPayload
    ) throws {
        if payload.formatVersion < payloadVersion {
            guard payload.capabilities.outboundGroups.isEmpty else {
                throw ProviderError.invalidSnapshot("旧版策略快照包含出口组目录")
            }
            return
        }
        var outbounds: [String: Nonproxy_Policy_V1_OutboundCapabilitySpec] = [:]
        for outbound in payload.capabilities.outbounds {
            outbounds[outbound.outboundID] = outbound
        }
        var groupIDs = Set<String>()
        var previousGroupID: String?
        for group in payload.capabilities.outboundGroups {
            let members = group.outboundIds.compactMap { outbounds[$0] }
            guard SnapshotContentValidator.isIdentifier(group.outboundGroupID),
                group.revision > 0,
                (2...32).contains(group.outboundIds.count),
                Set(group.outboundIds).count == group.outboundIds.count,
                members.count == group.outboundIds.count,
                groupIDs.insert(group.outboundGroupID).inserted,
                previousGroupID.map({ $0 < group.outboundGroupID }) ?? true,
                validOrderedUnique(group.transports, allowed: [.tcp, .udp]),
                validOrderedUnique(group.ipFamilies, allowed: [.ipv4, .ipv6]),
                group.transports == capabilityIntersection(members.map(\.transports)),
                group.ipFamilies == capabilityIntersection(members.map(\.ipFamilies))
            else {
                throw ProviderError.invalidSnapshot("策略快照出口组目录无效")
            }
            previousGroupID = group.outboundGroupID
        }
    }

    private static func capabilityIntersection<T: Hashable>(
        _ values: [[T]]
    ) -> [T] {
        guard let first = values.first else {
            return []
        }
        return first.filter { candidate in
            values.dropFirst().allSatisfy { $0.contains(candidate) }
        }
    }

    private static func unixMilliseconds(
        _ timestamp: Google_Protobuf_Timestamp
    ) throws -> UInt64 {
        guard timestamp.seconds >= 0,
              timestamp.nanos >= 0,
              timestamp.nanos < 1_000_000_000,
              timestamp.nanos % 1_000_000 == 0,
              let seconds = UInt64(exactly: timestamp.seconds)
        else {
            throw ProviderError.invalidSnapshot("策略时间戳无效")
        }
        let (milliseconds, multiplyOverflow) = seconds.multipliedReportingOverflow(by: 1_000)
        let (total, addOverflow) = milliseconds.addingReportingOverflow(
            UInt64(timestamp.nanos / 1_000_000)
        )
        guard !multiplyOverflow, !addOverflow, total > 0 else {
            throw ProviderError.invalidSnapshot("策略时间戳无效")
        }
        return total
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
            && left.outboundGroupID == right.outboundGroupID
            && left.failureMode == right.failureMode
    }

    private enum SHA256 {
        static let byteCount = 32
    }
}
