import Foundation
@testable import NonProxyDNSProxy
import NonProxyProviderContracts
import NonProxyProviderCore
import Synchronization
import XCTest

final class DNSQueryCoordinatorTests: XCTestCase {
    func testBuildsAuthenticatedResolverPayloadForDirectRoute() async throws {
        let runtime = try runtime(action: .direct)
        let resolver = RecordingDNSResolver()
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: runtime,
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [
                        DNSUpstreamEndpoint(ipAddress: "1.1.1.1"),
                    ]
                )
            ),
            networkProfile: DNSNetworkProfileMonitor(
                initialInterfaceIndex: 7
            ),
            decisions: decisions
        )
        let query = makeQuery()

        let response = try await coordinator.resolve(
            DNSFlowQueryContext(
                message: query,
                app: PolicyAppIdentity(
                    stableID: "com.example.client",
                    signerID: "TEAM"
                ),
                transport: .udp
            )
        )
        let recorded = await resolver.lastRequest

        XCTAssertFalse(response.isEmpty)
        XCTAssertEqual(recorded?.qname, "example.com")
        XCTAssertEqual(recorded?.qtype, 1)
        XCTAssertEqual(recorded?.requestedRoute, .direct)
        XCTAssertEqual(recorded?.snapshotVersion, 42)
        XCTAssertEqual(recorded?.directInterfaceIndex, 7)
        XCTAssertEqual(recorded?.app.stableID, "com.example.client")
        XCTAssertEqual(recorded?.app.signerID, "TEAM")
        XCTAssertEqual(recorded?.upstreams.first?.ipAddress, "1.1.1.1")
        XCTAssertTrue(recorded?.networkProfileID.hasPrefix("dns-") == true)
        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.level, .path)
        XCTAssertEqual(decisions.records[0].evidence.interfaceName, "ifindex:7")
    }

    func testBlockReturnsRefusedWithoutCallingResolver() async throws {
        let resolver = RecordingDNSResolver()
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: try runtime(action: .block),
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(upstreams: [])
            ),
            networkProfile: DNSNetworkProfileMonitor(),
            decisions: decisions
        )

        let response = try await coordinator.resolve(
            DNSFlowQueryContext(
                message: makeQuery(),
                app: .unknown,
                transport: .tcp
            )
        )

        XCTAssertEqual(readUInt16(response, at: 2) & 0x000F, 5)
        let recorded = await resolver.lastRequest
        XCTAssertNil(recorded)
        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.level, .decision)
    }

    func testProxyFailureFallsBackToObservedDirectPathWhenOpen() async throws {
        let resolver = RecordingDNSResolver(failProxy: true)
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: try runtime(
                action: .proxy,
                failureMode: .open
            ),
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [
                        DNSUpstreamEndpoint(ipAddress: "1.1.1.1"),
                    ]
                )
            ),
            networkProfile: DNSNetworkProfileMonitor(
                initialInterfaceIndex: 9
            ),
            decisions: decisions
        )

        _ = try await coordinator.resolve(
            DNSFlowQueryContext(
                message: makeQuery(),
                app: .unknown,
                transport: .udp
            )
        )

        let routes = await resolver.routes
        XCTAssertEqual(routes, [.proxy, .direct])
        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.level, .path)
        XCTAssertEqual(decisions.records[0].evidence.interfaceName, "ifindex:9")
        XCTAssertTrue(decisions.records[0].evidence.failOpenDirect)
        XCTAssertEqual(
            decisions.records[0].error.code,
            "NP_DNS_PROXY_FAIL_OPEN_DIRECT"
        )
    }

    func testCachedDnsResponseDoesNotClaimANewNetworkPath() async throws {
        let resolver = RecordingDNSResolver(cacheHit: true)
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: try runtime(action: .direct),
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [
                        DNSUpstreamEndpoint(ipAddress: "1.1.1.1"),
                    ]
                )
            ),
            networkProfile: DNSNetworkProfileMonitor(
                initialInterfaceIndex: 7
            ),
            decisions: decisions
        )

        _ = try await coordinator.resolve(
            DNSFlowQueryContext(
                message: makeQuery(),
                app: .unknown,
                transport: .udp
            )
        )

        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.level, .decision)
        XCTAssertTrue(decisions.records[0].evidence.interfaceName.isEmpty)
    }

    func testFailOpenCacheHitPreservesFailureWithoutClaimingAPath() async throws {
        let resolver = RecordingDNSResolver(
            failProxy: true,
            cacheHit: true
        )
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: try runtime(
                action: .proxy,
                failureMode: .open
            ),
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [
                        DNSUpstreamEndpoint(ipAddress: "1.1.1.1"),
                    ]
                )
            ),
            networkProfile: DNSNetworkProfileMonitor(
                initialInterfaceIndex: 7
            ),
            decisions: decisions
        )

        _ = try await coordinator.resolve(
            DNSFlowQueryContext(
                message: makeQuery(),
                app: .unknown,
                transport: .udp
            )
        )

        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.level, .decision)
        XCTAssertFalse(decisions.records[0].evidence.failOpenDirect)
        XCTAssertEqual(
            decisions.records[0].error.code,
            "NP_DNS_PROXY_FAIL_OPEN_CACHE_HIT"
        )
    }

    private func runtime(
        action: Nonproxy_Common_V1_RouteAction,
        failureMode: Nonproxy_Common_V1_FailureMode = .closed
    ) throws -> ProviderPolicyRuntime {
        var decision = Nonproxy_Policy_V1_DecisionSpec()
        decision.action = action
        decision.failureMode = failureMode
        decision.outboundID = action == .proxy ? "office" : ""
        var payload = Nonproxy_Policy_V1_CompiledPolicyPayload()
        payload.defaultDecision = decision
        var metadata = Nonproxy_Policy_V1_PolicySnapshotMetadata()
        metadata.snapshotVersion = 42
        var wire = Nonproxy_Policy_V1_CompiledPolicySnapshot()
        wire.metadata = metadata
        let runtime = ProviderPolicyRuntime()
        try runtime.install(
            VerifiedPolicySnapshot(wireSnapshot: wire, payload: payload)
        )
        return runtime
    }

    private func makeQuery() -> Data {
        Data([
            0x12, 0x34, 0x01, 0x00,
            0, 1, 0, 0, 0, 0, 0, 0,
            7, 101, 120, 97, 109, 112, 108, 101,
            3, 99, 111, 109, 0,
            0, 1, 0, 1,
        ])
    }

    private func readUInt16(_ data: Data, at offset: Int) -> UInt16 {
        UInt16(data[offset]) << 8 | UInt16(data[offset + 1])
    }
}

private actor RecordingDNSResolver: ProviderDNSResolving {
    private(set) var lastRequest: Nonproxy_Provider_V1_ResolveDnsRequest?
    private(set) var routes: [Nonproxy_Provider_V1_DnsRouteKind] = []
    private let failProxy: Bool
    private let cacheHit: Bool

    init(failProxy: Bool = false, cacheHit: Bool = false) {
        self.failProxy = failProxy
        self.cacheHit = cacheHit
    }

    func resolveDNS(
        _ request: Nonproxy_Provider_V1_ResolveDnsRequest
    ) async throws -> Nonproxy_Provider_V1_ResolveDnsResponse {
        lastRequest = request
        routes.append(request.requestedRoute)
        if failProxy, request.requestedRoute == .proxy {
            throw ProviderError.dnsResolution(
                code: "NP_TEST_PROXY_FAILED",
                message: "测试代理解析失败"
            )
        }
        let question = try DNSMessageParser.parseQuery(request.dnsMessage)
        var response = Nonproxy_Provider_V1_ResolveDnsResponse()
        response.dnsMessage = DNSResponseBuilder.refused(
            query: request.dnsMessage,
            question: question
        )
        response.route = request.requestedRoute
        response.outboundID = request.requestedOutboundID
        response.cacheHit = cacheHit
        return response
    }
}

private final class RecordingDecisionSubmitter:
    ProviderDecisionSubmitting,
    Sendable
{
    private let storage = Mutex<[Nonproxy_Provider_V1_DecisionRecord]>([])

    var records: [Nonproxy_Provider_V1_DecisionRecord] {
        storage.withLock { $0 }
    }

    func submit(
        _ decision: Nonproxy_Provider_V1_DecisionRecord
    ) -> Bool {
        storage.withLock { $0.append(decision) }
        return true
    }

    func recordUnreportable() {}
}
