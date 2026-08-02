import Foundation
@testable import NonProxyDNSProxy
import NonProxyMacNetworkIdentity
import NonProxyMacPlatformSupport
import NonProxyProviderContracts
import NonProxyProviderCore
import XCTest

final class DNSOutboundGroupCoordinatorTests: XCTestCase {
    func testRequestsTheGroupAndRecordsSelectedMember() async throws {
        let resolver = RecordingDNSResolver(selectedOutboundID: "backup")
        let decisions = RecordingDecisionSubmitter()
        let coordinator = makeCoordinator(
            runtime: try groupRuntime(failureMode: .closed),
            resolver: resolver,
            decisions: decisions
        )

        _ = try await coordinator.resolve(queryContext())

        let request = await resolver.lastRequest
        XCTAssertEqual(request?.requestedRoute, .proxy)
        XCTAssertTrue(request?.requestedOutboundID.isEmpty == true)
        XCTAssertEqual(request?.requestedOutboundGroupID, "automatic")
        XCTAssertEqual(request?.snapshotVersion, 42)
        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.outboundID, "backup")
    }

    func testRejectsASelectedMemberOutsideTheSnapshot() async throws {
        let resolver = RecordingDNSResolver(selectedOutboundID: "outsider")
        let decisions = RecordingDecisionSubmitter()
        let coordinator = makeCoordinator(
            runtime: try groupRuntime(failureMode: .closed),
            resolver: resolver,
            decisions: decisions
        )

        do {
            _ = try await coordinator.resolve(queryContext())
            XCTFail("出口组不得接受快照成员之外的实际出口")
        } catch {
            XCTAssertNotNil(error as? DNSProxyError)
        }

        XCTAssertEqual(decisions.records.count, 1)
        XCTAssertEqual(decisions.records[0].evidence.level, .decision)
        XCTAssertEqual(
            decisions.records[0].error.code,
            "NP_DNS_PROXY_RESOLVE_FAILED"
        )
    }

    func testFailureClearsTheGroupLabelBeforeFailOpenDirect() async throws {
        let resolver = RecordingDNSResolver(failProxy: true)
        let decisions = RecordingDecisionSubmitter()
        let coordinator = makeCoordinator(
            runtime: try groupRuntime(failureMode: .open),
            resolver: resolver,
            decisions: decisions
        )

        _ = try await coordinator.resolve(queryContext())

        let requests = await resolver.requests
        XCTAssertEqual(requests.count, 2)
        XCTAssertEqual(requests[0].requestedOutboundGroupID, "automatic")
        XCTAssertTrue(requests[1].requestedOutboundID.isEmpty)
        XCTAssertTrue(requests[1].requestedOutboundGroupID.isEmpty)
        XCTAssertEqual(requests[1].requestedRoute, .direct)
        XCTAssertTrue(decisions.records[0].evidence.failOpenDirect)
    }

    private func groupRuntime(
        failureMode: Nonproxy_Common_V1_FailureMode
    ) throws -> ProviderPolicyRuntime {
        var decision = Nonproxy_Policy_V1_DecisionSpec()
        decision.action = .proxy
        decision.outboundGroupID = "automatic"
        decision.failureMode = failureMode
        var group = Nonproxy_Policy_V1_OutboundGroupCapabilitySpec()
        group.outboundGroupID = "automatic"
        group.revision = 3
        group.outboundIds = ["primary", "backup"]
        group.transports = [.tcp, .udp]
        group.ipFamilies = [.ipv4, .ipv6]
        var primary = Nonproxy_Policy_V1_OutboundCapabilitySpec()
        primary.outboundID = "primary"
        primary.transports = [.tcp, .udp]
        primary.ipFamilies = [.ipv4, .ipv6]
        var backup = primary
        backup.outboundID = "backup"
        var capabilities = Nonproxy_Policy_V1_CompileCapabilitySet()
        capabilities.transports = [.tcp, .udp]
        capabilities.ipFamilies = [.ipv4, .ipv6]
        capabilities.outbounds = [backup, primary]
        capabilities.outboundGroups = [group]
        var payload = Nonproxy_Policy_V1_CompiledPolicyPayload()
        payload.formatVersion = 4
        payload.capabilities = capabilities
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

    private func makeCoordinator(
        runtime: ProviderPolicyRuntime,
        resolver: any ProviderDNSResolving,
        decisions: any ProviderDecisionSubmitting
    ) -> DNSQueryCoordinator {
        DNSQueryCoordinator(
            runtime: runtime,
            resolver: resolver,
            catalogs: DNSResolverCatalogStore(
                DNSSystemResolverCatalog(
                    upstreams: [DNSUpstreamEndpoint(ipAddress: "1.1.1.1")]
                )
            ),
            networkEnvironment: MacNetworkEnvironmentMonitor(
                initialInterfaceIndex: 9
            ),
            decisions: decisions
        )
    }

    private func queryContext() -> DNSFlowQueryContext {
        DNSFlowQueryContext(
            message: Data([
                0x12, 0x34, 0x01, 0x00,
                0, 1, 0, 0, 0, 0, 0, 0,
                7, 101, 120, 97, 109, 112, 108, 101,
                3, 99, 111, 109, 0,
                0, 1, 0, 1,
            ]),
            app: .unknown,
            transport: .udp
        )
    }
}
