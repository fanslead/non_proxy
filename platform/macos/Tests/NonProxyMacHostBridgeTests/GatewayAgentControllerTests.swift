import ServiceManagement
import Testing
@testable import NonProxyMacHostBridge

@MainActor
struct GatewayAgentControllerTests {
    @Test
    func mapsEnabledAgentReadiness() {
        let waiting = GatewayAgentController.snapshot(
            status: .enabled,
            runtimeReady: false
        )
        let ready = GatewayAgentController.snapshot(
            status: .enabled,
            runtimeReady: true
        )
        let upgrade = GatewayAgentController.snapshot(
            status: .enabled,
            runtimeReady: false,
            requiresUpgrade: true
        )

        #expect(waiting.registered)
        #expect(waiting.enabled)
        #expect(!waiting.ready)
        #expect(ready.ready)
        #expect(upgrade.requiresUpgrade)
    }

    @Test
    func mapsApprovalAndMissingPackageSeparately() {
        let approval = GatewayAgentController.snapshot(
            status: .requiresApproval,
            runtimeReady: false
        )
        let missing = GatewayAgentController.snapshot(
            status: .notFound,
            runtimeReady: false
        )

        #expect(approval.registered)
        #expect(approval.requiresApproval)
        #expect(approval.found)
        #expect(!missing.registered)
        #expect(!missing.found)
    }

    @Test
    func replacementStopsNetworkBeforeReplacingAgent() async throws {
        let events = GatewayAgentTestEvents()
        let service = FakeGatewayAgentService(
            status: .enabled,
            events: events
        )
        let controller = GatewayAgentController(
            service: service,
            appGroupValidator: {},
            fingerprintProvider: {
                String(repeating: "a", count: 64)
            },
            runtimeInspector: { _ in
                events.runtimeChecks += 1
                return events.runtimeChecks == 1
                    ? .notReady
                    : .ready
            }
        )

        let outcome = try await controller.registerAndWait(
            approvalHandler: {},
            prepareForReplacement: {
                events.values.append("prepare-network")
            }
        )

        #expect(!outcome.newlyRegistered)
        #expect(
            events.values
                == ["prepare-network", "unregister", "register"]
        )
    }

    @Test
    func freshRegistrationRemainsRollbackEligible() async throws {
        let events = GatewayAgentTestEvents()
        let service = FakeGatewayAgentService(
            status: .notRegistered,
            events: events
        )
        let controller = GatewayAgentController(
            service: service,
            appGroupValidator: {},
            fingerprintProvider: {
                String(repeating: "a", count: 64)
            },
            runtimeInspector: { _ in .ready }
        )

        let outcome = try await controller.registerAndWait(
            approvalHandler: {},
            prepareForReplacement: {
                events.values.append("unexpected-prepare")
            }
        )

        #expect(outcome.newlyRegistered)
        #expect(events.values == ["register"])
    }
}

@MainActor
private final class GatewayAgentTestEvents {
    var values: [String] = []
    var runtimeChecks = 0
}

@MainActor
private final class FakeGatewayAgentService: GatewayAgentServicing {
    var status: SMAppService.Status
    private let events: GatewayAgentTestEvents

    init(
        status: SMAppService.Status,
        events: GatewayAgentTestEvents
    ) {
        self.status = status
        self.events = events
    }

    func register() throws {
        events.values.append("register")
        status = .enabled
    }

    func unregister() async throws {
        events.values.append("unregister")
        status = .notRegistered
    }
}
