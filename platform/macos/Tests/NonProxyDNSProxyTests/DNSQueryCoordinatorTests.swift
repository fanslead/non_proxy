import Foundation
@testable import NonProxyDNSProxy
import NonProxyMacNetworkIdentity
import NonProxyMacPlatformSupport
import NonProxyProviderContracts
import NonProxyProviderCore
import SwiftProtobuf
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
            networkEnvironment: MacNetworkEnvironmentMonitor(
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
            networkEnvironment: MacNetworkEnvironmentMonitor(),
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

    func testPausedOverrideUsesSystemDnsWithoutClaimingPolicyEvidence() async throws {
        let resolver = RecordingDNSResolver()
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: try pausedRuntime(),
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [
                        DNSUpstreamEndpoint(ipAddress: "1.1.1.1"),
                    ]
                )
            ),
            networkEnvironment: MacNetworkEnvironmentMonitor(
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

        let request = await resolver.lastRequest
        XCTAssertEqual(request?.requestedRoute, .system)
        XCTAssertEqual(request?.snapshotVersion, 42)
        XCTAssertEqual(request?.directInterfaceIndex, 0)
        XCTAssertTrue(request?.requestedOutboundID.isEmpty == true)
        XCTAssertTrue(decisions.records.isEmpty)
    }

    func testUsesResolvedNetworkProfileForDnsPolicyDecision() async throws {
        let fingerprint = try XCTUnwrap(
            MacNetworkFingerprintFactory.interfaceClass("wifi")
        )
        let resolver = RecordingDNSResolver()
        let decisions = RecordingDecisionSubmitter()
        let coordinator = DNSQueryCoordinator(
            runtime: try networkRuntime(fingerprint: fingerprint),
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [
                        DNSUpstreamEndpoint(ipAddress: "1.1.1.1"),
                    ]
                )
            ),
            networkEnvironment: MacNetworkEnvironmentMonitor(
                initialInterfaceIndex: 7,
                initialFingerprints: [fingerprint]
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

        let request = await resolver.lastRequest
        XCTAssertNotNil(request)
        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].context.networkProfileID, "office")
        XCTAssertEqual(decisions.records[0].decision.matchedPolicyID, "office-direct")
        XCTAssertEqual(decisions.records[0].decision.result.action, .direct)
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
            networkEnvironment: MacNetworkEnvironmentMonitor(
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
        let requests = await resolver.requests
        XCTAssertTrue(requests[1].requestedOutboundID.isEmpty)
        XCTAssertTrue(requests[1].requestedOutboundGroupID.isEmpty)
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
            networkEnvironment: MacNetworkEnvironmentMonitor(
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
            networkEnvironment: MacNetworkEnvironmentMonitor(
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

    private func pausedRuntime() throws -> ProviderPolicyRuntime {
        var blocked = Nonproxy_Policy_V1_DecisionSpec()
        blocked.action = .block
        blocked.failureMode = .closed
        var runtimeOverride = Nonproxy_Policy_V1_RuntimeRoutingOverride()
        runtimeOverride.mode = .paused
        var expiresAt = Google_Protobuf_Timestamp()
        expiresAt.seconds = 4_102_444_800
        runtimeOverride.expiresAt = expiresAt
        var payload = Nonproxy_Policy_V1_CompiledPolicyPayload()
        payload.formatVersion = 3
        payload.defaultDecision = blocked
        payload.runtimeOverride = runtimeOverride
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

    private func networkRuntime(
        fingerprint: MacNetworkFingerprint
    ) throws -> ProviderPolicyRuntime {
        var profile = Nonproxy_Policy_V1_NetworkProfileBinding()
        profile.id = "office"
        profile.fingerprintKind = .interfaceClass
        profile.fingerprintValue = fingerprint.value

        var network = Nonproxy_Policy_V1_NetworkMatcher()
        network.profileID = profile.id
        var matcher = Nonproxy_Policy_V1_PolicyMatch()
        matcher.network = network
        var direct = Nonproxy_Policy_V1_DecisionSpec()
        direct.action = .direct
        direct.failureMode = .closed
        var policy = Nonproxy_Policy_V1_Policy()
        policy.id = "office-direct"
        policy.sourceKind = .network
        policy.match = matcher
        policy.decision = direct
        policy.enabled = true

        var blocked = Nonproxy_Policy_V1_DecisionSpec()
        blocked.action = .block
        blocked.failureMode = .closed
        var payload = Nonproxy_Policy_V1_CompiledPolicyPayload()
        payload.formatVersion = 2
        payload.policies = [policy]
        payload.defaultDecision = blocked
        payload.networkProfiles = [profile]
        var metadata = Nonproxy_Policy_V1_PolicySnapshotMetadata()
        metadata.snapshotVersion = 43
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
