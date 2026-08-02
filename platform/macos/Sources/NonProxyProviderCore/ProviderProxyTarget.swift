import NonProxyProviderContracts

public enum ProviderProxyTarget: Sendable, Equatable {
    case outbound(id: String)
    case group(
        id: String,
        snapshotVersion: UInt64,
        memberIDs: [String]
    )

    public var isValid: Bool {
        switch self {
        case .outbound(let id):
            SnapshotContentValidator.isIdentifier(id)
        case .group(let id, let snapshotVersion, let memberIDs):
            SnapshotContentValidator.isIdentifier(id)
                && snapshotVersion > 0
                && (2...32).contains(memberIDs.count)
                && Set(memberIDs).count == memberIDs.count
                && memberIDs.allSatisfy(SnapshotContentValidator.isIdentifier)
        }
    }

    public func accepts(selectedOutboundID: String) -> Bool {
        guard isValid else {
            return false
        }
        return switch self {
        case .outbound(let id):
            id == selectedOutboundID
        case .group(_, _, let memberIDs):
            memberIDs.contains(selectedOutboundID)
        }
    }

    public func matches(decision: PolicyDecision) -> Bool {
        guard isValid else {
            return false
        }
        switch self {
        case .outbound(let id):
            return decision.result.outboundID == id
                && decision.result.outboundGroupID.isEmpty
        case .group(let id, let snapshotVersion, _):
            return decision.result.outboundID.isEmpty
                && decision.result.outboundGroupID == id
                && decision.snapshotVersion == snapshotVersion
        }
    }
}
