import Foundation
import NonProxyProviderContracts
import Synchronization
import SwiftProtobuf

public final class ProviderPolicyRuntime: Sendable {
    private struct State: Sendable {
        var snapshot: VerifiedPolicySnapshot?
    }

    private let state = Mutex(State())

    public init() {}

    public var activeSnapshotVersion: UInt64 {
        state.withLock { $0.snapshot?.version ?? 0 }
    }

    @discardableResult
    public func install(_ snapshot: VerifiedPolicySnapshot) throws -> Bool {
        try state.withLock { state in
            guard let current = state.snapshot else {
                state.snapshot = snapshot
                return true
            }
            guard snapshot.version >= current.version else {
                throw ProviderError.invalidSnapshot("Provider 拒绝降级策略快照")
            }
            if snapshot.version == current.version {
                guard snapshot.contentHash == current.contentHash else {
                    throw ProviderError.invalidSnapshot("相同版本的策略快照哈希不一致")
                }
                return false
            }
            state.snapshot = snapshot
            return true
        }
    }

    public func decide(
        context: PolicyConnectionContext
    ) throws -> PolicyDecision {
        guard let decision = try evaluate(context: context).decision else {
            throw ProviderError.lifecycle("暂停期间没有策略决策")
        }
        return decision
    }

    public func evaluate(
        context: PolicyConnectionContext,
        networkFingerprints: [PolicyNetworkFingerprint] = [],
        at date: Date = Date()
    ) throws -> ProviderPolicyEvaluation {
        let snapshot = state.withLock { $0.snapshot }
        guard let snapshot else {
            throw ProviderError.lifecycle("Provider 尚无可用策略快照")
        }
        let resolvedContext = PolicyConnectionContext(
            app: context.app,
            destination: context.destination,
            networkProfileID: context.networkProfileID
                ?? PolicyNetworkProfileResolver.resolve(
                    in: snapshot,
                    fingerprints: networkFingerprints
                )
        )
        let disposition: ProviderPolicyDisposition
        if let system = ProviderPolicyEngine.decideSystem(
            snapshot: snapshot,
            context: resolvedContext
        ) {
            disposition = .decision(system)
        } else if snapshot.payload.hasRuntimeOverride,
                  isActive(snapshot.payload.runtimeOverride, at: date) {
            disposition = try runtimeDisposition(
                snapshot.payload.runtimeOverride,
                snapshotVersion: snapshot.version
            )
        } else {
            disposition = .decision(
                ProviderPolicyEngine.decideAfterSystem(
                    snapshot: snapshot,
                    context: resolvedContext
                )
            )
        }
        return ProviderPolicyEvaluation(
            context: resolvedContext,
            disposition: disposition
        )
    }

    private func isActive(
        _ runtimeOverride: Nonproxy_Policy_V1_RuntimeRoutingOverride,
        at date: Date
    ) -> Bool {
        guard let now = Self.unixMilliseconds(date),
              let expiresAt = Self.unixMilliseconds(runtimeOverride.expiresAt)
        else {
            return false
        }
        return now < expiresAt
    }

    private func runtimeDisposition(
        _ runtimeOverride: Nonproxy_Policy_V1_RuntimeRoutingOverride,
        snapshotVersion: UInt64
    ) throws -> ProviderPolicyDisposition {
        switch runtimeOverride.mode {
        case .paused:
            return .bypass(
                snapshotVersion: snapshotVersion,
                reasonCode: "NP_RUNTIME_OVERRIDE_PAUSED"
            )
        case .direct:
            var decision = Nonproxy_Policy_V1_DecisionSpec()
            decision.action = .direct
            decision.failureMode = .closed
            return .decision(
                PolicyDecision(
                    result: decision,
                    matchedPolicyID: nil,
                    snapshotVersion: snapshotVersion,
                    reasonCode: "NP_RUNTIME_OVERRIDE_DIRECT"
                )
            )
        case .proxy:
            var decision = Nonproxy_Policy_V1_DecisionSpec()
            decision.action = .proxy
            decision.outboundID = runtimeOverride.outboundID
            decision.failureMode = .closed
            return .decision(
                PolicyDecision(
                    result: decision,
                    matchedPolicyID: nil,
                    snapshotVersion: snapshotVersion,
                    reasonCode: "NP_RUNTIME_OVERRIDE_PROXY"
                )
            )
        default:
            throw ProviderError.invalidSnapshot("运行态覆盖模式无效")
        }
    }

    private static func unixMilliseconds(_ date: Date) -> UInt64? {
        let milliseconds = date.timeIntervalSince1970 * 1_000
        guard milliseconds.isFinite,
              milliseconds >= 0,
              milliseconds < Double(UInt64.max)
        else {
            return nil
        }
        return UInt64(milliseconds.rounded(.towardZero))
    }

    private static func unixMilliseconds(
        _ timestamp: Google_Protobuf_Timestamp
    ) -> UInt64? {
        guard timestamp.seconds >= 0,
              timestamp.nanos >= 0,
              timestamp.nanos < 1_000_000_000,
              timestamp.nanos % 1_000_000 == 0,
              let seconds = UInt64(exactly: timestamp.seconds)
        else {
            return nil
        }
        let (milliseconds, multiplyOverflow) = seconds.multipliedReportingOverflow(by: 1_000)
        guard !multiplyOverflow else {
            return nil
        }
        let (total, addOverflow) = milliseconds.addingReportingOverflow(
            UInt64(timestamp.nanos / 1_000_000)
        )
        return addOverflow ? nil : total
    }
}

public enum ProviderPolicyDisposition: Sendable {
    case bypass(snapshotVersion: UInt64, reasonCode: String)
    case decision(PolicyDecision)
}

public struct ProviderPolicyEvaluation: Sendable {
    public let context: PolicyConnectionContext
    public let disposition: ProviderPolicyDisposition

    public var decision: PolicyDecision? {
        guard case .decision(let decision) = disposition else {
            return nil
        }
        return decision
    }

    public init(
        context: PolicyConnectionContext,
        disposition: ProviderPolicyDisposition
    ) {
        self.context = context
        self.disposition = disposition
    }
}
