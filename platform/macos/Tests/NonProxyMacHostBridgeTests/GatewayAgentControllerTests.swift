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
    func queryPreservesInstallationEligibilityError() {
        let events = GatewayAgentTestEvents()
        let service = FakeGatewayAgentService(
            status: .notFound,
            events: events
        )
        let controller = BackgroundAgentController(
            descriptor: .gateway,
            service: service,
            installationValidator: {
                throw BridgeError(
                    code: "NP_MAC_MISSING_ENTITLEMENT",
                    message: "测试 Provisioning Profile 缺失"
                )
            },
            fingerprintProvider: {
                String(repeating: "a", count: 64)
            },
            runtimeInspector: { _ in .notReady }
        )

        do {
            _ = try controller.query()
            Issue.record("签名能力不足时不应继续报告后台项目状态")
        } catch let error as BridgeError {
            #expect(error.code == "NP_MAC_MISSING_ENTITLEMENT")
            #expect(error.message.contains("Provisioning Profile"))
        } catch {
            Issue.record("返回了非产品错误：\(error)")
        }
    }

    @Test
    func queryMapsUnavailableAppGroupBeforeReportingMissingPackage() {
        let events = GatewayAgentTestEvents()
        let service = FakeGatewayAgentService(
            status: .notFound,
            events: events
        )
        let controller = BackgroundAgentController(
            descriptor: .gateway,
            service: service,
            installationValidator: {
                throw GatewayAgentTestError.appGroupUnavailable
            },
            fingerprintProvider: {
                String(repeating: "a", count: 64)
            },
            runtimeInspector: { _ in .notReady }
        )

        do {
            _ = try controller.query()
            Issue.record("App Group 不可用时不应继续报告后台项目状态")
        } catch let error as BridgeError {
            #expect(error.code == "NP_MAC_APP_GROUP_UNAVAILABLE")
            #expect(error.message.contains("App Group"))
        } catch {
            Issue.record("返回了非产品错误：\(error)")
        }
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
            installationValidator: {},
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
            installationValidator: {},
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

    @Test
    func adapterHostUsesTheSameBoundedRegistrationStateMachine() async throws {
        let events = GatewayAgentTestEvents()
        let service = FakeGatewayAgentService(
            status: .notRegistered,
            events: events
        )
        let controller = AdapterHostAgentController(
            service: service,
            installationValidator: {},
            fingerprintProvider: {
                String(repeating: "b", count: 64)
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

    @Test
    func freshAdapterHostRegistrationRollsBackWhenReadinessFails() async {
        let events = GatewayAgentTestEvents()
        let service = FakeGatewayAgentService(
            status: .notRegistered,
            events: events
        )
        let controller = BackgroundAgentController(
            descriptor: .adapterHost,
            service: service,
            installationValidator: {},
            fingerprintProvider: {
                String(repeating: "b", count: 64)
            },
            runtimeInspector: { _ in .notReady },
            readinessAttempts: 1,
            readinessDelay: .zero
        )

        do {
            _ = try await controller.registerAndWait(
                approvalHandler: {},
                prepareForReplacement: {}
            )
            Issue.record("未就绪的 adapter-host 不应登记成功")
        } catch let error as BridgeError {
            #expect(error.code == "NP_MAC_ADAPTER_HOST_NOT_READY")
        } catch {
            Issue.record("返回了非产品错误：\(error)")
        }

        #expect(events.values == ["register", "unregister"])
        #expect(service.status == .notRegistered)
    }
}

@MainActor
private final class GatewayAgentTestEvents {
    var values: [String] = []
    var runtimeChecks = 0
}

private enum GatewayAgentTestError: Error {
    case appGroupUnavailable
}

@MainActor
private final class FakeGatewayAgentService: BackgroundAgentServicing {
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
