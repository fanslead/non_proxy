import Foundation
import Synchronization

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
        try evaluate(context: context).decision
    }

    public func evaluate(
        context: PolicyConnectionContext,
        networkFingerprints: [PolicyNetworkFingerprint] = []
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
        let decision = ProviderPolicyEngine.decide(
            snapshot: snapshot,
            context: resolvedContext
        )
        return ProviderPolicyEvaluation(
            context: resolvedContext,
            decision: decision
        )
    }
}

public struct ProviderPolicyEvaluation: Sendable {
    public let context: PolicyConnectionContext
    public let decision: PolicyDecision

    public init(
        context: PolicyConnectionContext,
        decision: PolicyDecision
    ) {
        self.context = context
        self.decision = decision
    }
}
