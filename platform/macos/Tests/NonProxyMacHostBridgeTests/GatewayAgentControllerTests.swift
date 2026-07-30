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

        #expect(waiting.registered)
        #expect(waiting.enabled)
        #expect(!waiting.ready)
        #expect(ready.ready)
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
}
