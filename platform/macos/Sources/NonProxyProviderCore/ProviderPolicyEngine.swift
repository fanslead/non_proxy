import Foundation
import Network
import NonProxyProviderContracts

public enum ProviderPolicyEngine {
    public static func decide(
        snapshot: VerifiedPolicySnapshot,
        context: PolicyConnectionContext
    ) -> PolicyDecision {
        if let system = decideSystem(snapshot: snapshot, context: context) {
            return system
        }
        return decideAfterSystem(snapshot: snapshot, context: context)
    }

    static func decideSystem(
        snapshot: VerifiedPolicySnapshot,
        context: PolicyConnectionContext
    ) -> PolicyDecision? {
        bestDecision(snapshot: snapshot, context: context, tier: .system)
    }

    static func decideAfterSystem(
        snapshot: VerifiedPolicySnapshot,
        context: PolicyConnectionContext
    ) -> PolicyDecision {
        for tier in RuleTier.allCases.reversed() where tier != .system {
            if let decision = bestDecision(
                snapshot: snapshot,
                context: context,
                tier: tier
            ) {
                return decision
            }
        }
        return PolicyDecision(
            result: snapshot.payload.defaultDecision,
            matchedPolicyID: nil,
            snapshotVersion: snapshot.version,
            reasonCode: "NP_POLICY_DEFAULT"
        )
    }

    private static func bestDecision(
        snapshot: VerifiedPolicySnapshot,
        context: PolicyConnectionContext,
        tier: RuleTier
    ) -> PolicyDecision? {
            let candidates = snapshot.payload.policies.filter {
                ruleTier(for: $0) == tier && matches($0.match, context: context)
            }
            if let selected = candidates.reduce(nil, prefer) {
                return PolicyDecision(
                    result: selected.decision,
                    matchedPolicyID: selected.id,
                    matchedRuleID: selected.id,
                    snapshotVersion: snapshot.version,
                    reasonCode: tier.reasonCode
                )
            }
        return nil
    }

    private static func matches(
        _ matcher: Nonproxy_Policy_V1_PolicyMatch,
        context: PolicyConnectionContext
    ) -> Bool {
        if !matcher.transports.isEmpty,
           !matcher.transports.contains(context.destination.transport) {
            return false
        }
        if !matcher.ports.isEmpty,
           !matcher.ports.contains(where: {
               $0.first <= UInt32(context.destination.port)
                   && UInt32(context.destination.port) <= $0.last
           }) {
            return false
        }
        if matcher.hasApp, !matches(matcher.app, identity: context.app) {
            return false
        }
        if matcher.hasDomain,
           !matches(matcher.domain, destination: context.destination) {
            return false
        }
        if matcher.hasCidr,
           !matches(matcher.cidr, ipAddress: context.destination.ipAddress) {
            return false
        }
        if matcher.hasNetwork,
           matcher.network.profileID != context.networkProfileID {
            return false
        }
        return true
    }

    private static func matches(
        _ matcher: Nonproxy_Policy_V1_AppMatcher,
        identity: PolicyAppIdentity
    ) -> Bool {
        guard matcher.platform == .macos else {
            return false
        }
        let stableMatches = matcher.stableID == identity.stableID
            || (
                matcher.includeHelpers
                    && (
                        matcher.stableID == identity.parentStableID
                            || matcher.stableID == identity.helperGroupID
                    )
            )
        guard stableMatches else {
            return false
        }
        return matcher.signerID.isEmpty || matcher.signerID == identity.signerID
    }

    private static func matches(
        _ matcher: Nonproxy_Policy_V1_DomainMatcher,
        destination: PolicyDestination
    ) -> Bool {
        guard let domain = destination.normalizedDomain else {
            return false
        }
        switch matcher.kind {
        case .exact:
            return domain == matcher.asciiPattern
        case .suffix:
            return domain == matcher.asciiPattern
                || domain.hasSuffix(".\(matcher.asciiPattern)")
        case .registrableDomain:
            // 编译器已证明模式本身是可注册域；其子域拥有同一可注册域。
            return domain == matcher.asciiPattern
                || domain.hasSuffix(".\(matcher.asciiPattern)")
        default:
            return false
        }
    }

    private static func matches(
        _ matcher: Nonproxy_Policy_V1_CidrMatcher,
        ipAddress: String?
    ) -> Bool {
        guard let ipAddress else {
            return false
        }
        if let network = IPv4Address(matcher.network),
           let address = IPv4Address(ipAddress),
           matcher.prefixLength <= 32 {
            return prefixMatches(
                network.rawValue,
                address.rawValue,
                prefixLength: Int(matcher.prefixLength)
            )
        }
        if let network = IPv6Address(matcher.network),
           let address = IPv6Address(ipAddress),
           matcher.prefixLength <= 128 {
            return prefixMatches(
                network.rawValue,
                address.rawValue,
                prefixLength: Int(matcher.prefixLength)
            )
        }
        return false
    }

    private static func prefixMatches(
        _ network: Data,
        _ address: Data,
        prefixLength: Int
    ) -> Bool {
        guard network.count == address.count else {
            return false
        }
        let fullBytes = prefixLength / 8
        guard network.prefix(fullBytes) == address.prefix(fullBytes) else {
            return false
        }
        let remainingBits = prefixLength % 8
        guard remainingBits > 0 else {
            return true
        }
        let mask = UInt8.max << (8 - remainingBits)
        return network[fullBytes] & mask == address[fullBytes] & mask
    }

    private static func prefer(
        _ current: Nonproxy_Policy_V1_Policy?,
        _ candidate: Nonproxy_Policy_V1_Policy
    ) -> Nonproxy_Policy_V1_Policy? {
        guard let current else {
            return candidate
        }
        if candidate.priority != current.priority {
            return candidate.priority > current.priority ? candidate : current
        }
        let candidateSpecificity = specificity(candidate.match)
        let currentSpecificity = specificity(current.match)
        if candidateSpecificity != currentSpecificity {
            return candidateSpecificity > currentSpecificity ? candidate : current
        }
        return candidate.id < current.id ? candidate : current
    }

    private static func specificity(
        _ matcher: Nonproxy_Policy_V1_PolicyMatch
    ) -> RuleSpecificity {
        let appSigner = matcher.hasApp && !matcher.app.signerID.isEmpty ? 1 : 0
        let destination: (Int, Int)
        if matcher.hasDomain {
            let kind: Int
            switch matcher.domain.kind {
            case .suffix:
                kind = 2
            case .registrableDomain:
                kind = 3
            case .exact:
                kind = 4
            default:
                kind = 0
            }
            destination = (kind, matcher.domain.asciiPattern.split(separator: ".").count)
        } else if matcher.hasCidr {
            destination = (1, Int(matcher.cidr.prefixLength))
        } else {
            destination = (0, 0)
        }
        let coveredPorts = matcher.ports.reduce(UInt32(0)) {
            $0 + ($1.last - $1.first + 1)
        }
        return RuleSpecificity(
            appSigner: appSigner,
            destinationKind: destination.0,
            destinationDepth: destination.1,
            transport: matcher.transports.isEmpty ? 0 : 1,
            portNarrowness: matcher.ports.isEmpty
                ? 0
                : Int(UInt32(UInt16.max) + 1 - coveredPorts)
        )
    }

    private static func ruleTier(
        for policy: Nonproxy_Policy_V1_Policy
    ) -> RuleTier {
        switch policy.sourceKind {
        case .system:
            return .system
        case .appDestination:
            return .appDestination
        case .app:
            return .app
        case .site, .cidr:
            return .destination
        case .network:
            return .network
        case .builtIn:
            return .builtIn
        case .adapter:
            if policy.match.hasApp {
                return policy.match.hasDomain || policy.match.hasCidr
                    ? .appDestination
                    : .app
            }
            return .destination
        default:
            return .destination
        }
    }
}

private enum RuleTier: Int, CaseIterable {
    case builtIn = 1
    case network
    case destination
    case app
    case appDestination
    case system

    var reasonCode: String {
        switch self {
        case .system:
            "NP_POLICY_SYSTEM_MATCH"
        case .appDestination:
            "NP_POLICY_APP_DESTINATION_MATCH"
        case .app:
            "NP_POLICY_APP_MATCH"
        case .destination:
            "NP_POLICY_DESTINATION_MATCH"
        case .network:
            "NP_POLICY_NETWORK_MATCH"
        case .builtIn:
            "NP_POLICY_BUILTIN_MATCH"
        }
    }
}

private struct RuleSpecificity: Equatable, Comparable {
    let appSigner: Int
    let destinationKind: Int
    let destinationDepth: Int
    let transport: Int
    let portNarrowness: Int

    static func < (left: Self, right: Self) -> Bool {
        [
            left.appSigner,
            left.destinationKind,
            left.destinationDepth,
            left.transport,
            left.portNarrowness,
        ].lexicographicallyPrecedes([
            right.appSigner,
            right.destinationKind,
            right.destinationDepth,
            right.transport,
            right.portNarrowness,
        ])
    }
}
