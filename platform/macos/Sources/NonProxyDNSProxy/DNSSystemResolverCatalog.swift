import Foundation
import NetworkExtension
import Synchronization

public struct DNSSystemResolverCatalog: Sendable {
    private struct ResolverRule: Sendable {
        let matchDomains: [String]
        let upstreams: [DNSUpstreamEndpoint]
    }

    private let rules: [ResolverRule]

    public init(settings: [NEDNSSettings]) {
        rules = settings.compactMap { setting in
            guard setting.dnsProtocol == .cleartext else {
                return nil
            }
            let upstreams = setting.servers.compactMap(
                DNSUpstreamEndpoint.parse
            )
            guard !upstreams.isEmpty else {
                return nil
            }
            let domains = (setting.matchDomains ?? [])
                .compactMap(Self.normalizeDomain)
            return ResolverRule(
                matchDomains: domains,
                upstreams: upstreams
            )
        }
    }

    public init(upstreams: [DNSUpstreamEndpoint]) {
        rules = upstreams.isEmpty
            ? []
            : [ResolverRule(matchDomains: [], upstreams: upstreams)]
    }

    public func upstreams(for qname: String) -> [DNSUpstreamEndpoint] {
        let matches = rules.compactMap { rule -> (
            specificity: Int,
            upstreams: [DNSUpstreamEndpoint]
        )? in
            if rule.matchDomains.isEmpty {
                return (0, rule.upstreams)
            }
            let specificity = rule.matchDomains
                .filter { Self.matches(qname, domain: $0) }
                .map(\.utf8.count)
                .max()
            return specificity.map { ($0, rule.upstreams) }
        }
        guard let best = matches.map(\.specificity).max() else {
            return []
        }
        var seen: Set<DNSUpstreamEndpoint> = []
        return matches
            .filter { $0.specificity == best }
            .flatMap(\.upstreams)
            .filter { seen.insert($0).inserted }
    }

    private static func normalizeDomain(_ value: String) -> String? {
        var candidate = value.lowercased()
        while candidate.hasSuffix(".") {
            candidate.removeLast()
        }
        guard candidate == candidate.trimmingCharacters(
            in: .whitespacesAndNewlines
        ) else {
            return nil
        }
        return candidate
    }

    private static func matches(_ qname: String, domain: String) -> Bool {
        domain.isEmpty
            || qname == domain
            || qname.hasSuffix(".\(domain)")
    }
}

public final class DNSResolverCatalogStore: Sendable {
    private let catalog: Mutex<DNSSystemResolverCatalog>

    public init(_ catalog: DNSSystemResolverCatalog) {
        self.catalog = Mutex(catalog)
    }

    public func replace(with value: DNSSystemResolverCatalog) {
        catalog.withLock { $0 = value }
    }

    public func upstreams(for qname: String) -> [DNSUpstreamEndpoint] {
        catalog.withLock { $0.upstreams(for: qname) }
    }
}
