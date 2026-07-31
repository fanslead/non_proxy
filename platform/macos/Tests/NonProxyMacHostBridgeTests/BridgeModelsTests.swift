import Foundation
import NonProxyMacNetworkIdentity
import SystemExtensions
import Testing

@testable import NonProxyMacHostBridge

struct BridgeModelsTests {
    @Test
    func probePayloadPreservesUtf8Text() throws {
        let payload = ProbePayload(
            abiVersion: BridgeConstants.abiVersion,
            message: "NonProxy 原生桥接已连接"
        )

        let data = try JSONEncoder().encode(payload)
        let decoded = try JSONDecoder().decode(ProbePayload.self, from: data)

        #expect(decoded == payload)
        #expect(String(decoding: data, as: UTF8.self).contains("原生桥接"))
    }

    @Test
    func failurePayloadKeepsStableErrorCode() {
        let error = BridgeError(
            code: "NP_MAC_TEST_FAILURE",
            message: "测试错误"
        )

        let payload = BridgeEventPayload.failure(
            operation: .installAndEnable,
            error: error
        )

        #expect(!payload.success)
        #expect(payload.errorCode == "NP_MAC_TEST_FAILURE")
        #expect(payload.message == "测试错误")
    }

    @Test
    func applicationCatalogPreservesSigningIdentityAndUtf8Name() throws {
        let application = ApplicationDescriptor(
            displayName: "企业办公",
            stableIdentity: "com.example.office",
            signerIdentity: "TEAM123",
            bundleIdentifier: "com.example.office",
            isRunning: true
        )
        let payload = ApplicationCatalogPayload.result(
            applications: [application]
        )

        let data = try JSONEncoder().encode(payload)
        let decoded = try JSONDecoder().decode(
            ApplicationCatalogPayload.self,
            from: data
        )

        #expect(decoded == payload)
        #expect(decoded.applications.first?.stableIdentity == "com.example.office")
        #expect(String(decoding: data, as: UTF8.self).contains("企业办公"))
    }

    @Test
    func currentNetworkPayloadExposesOnlyPrivacySafeFingerprint() throws {
        let fingerprint = try #require(
            MacNetworkFingerprintFactory.wifiSSID("Office WiFi")
        )

        let payload = CurrentNetworkPayload.result(
            fingerprints: [fingerprint],
            permission: .authorized
        )
        let data = try JSONEncoder().encode(payload)
        let json = String(decoding: data, as: UTF8.self)

        #expect(payload.success)
        #expect(payload.fingerprint?.kind == .wifiSSIDHash)
        #expect(payload.fingerprint?.value.count == 64)
        #expect(!json.contains("Office WiFi"))
        #expect(json.contains("wifi_ssid_sha256"))
    }

    @Test
    func currentNetworkPayloadExplainsGatewayFallbackAfterDenial() throws {
        let fingerprint = try #require(
            MacNetworkFingerprintFactory.defaultGateway("192.168.1.1")
        )

        let payload = CurrentNetworkPayload.result(
            fingerprints: [fingerprint],
            permission: .denied
        )

        #expect(payload.success)
        #expect(payload.message.contains("定位权限不可用"))
        #expect(payload.suggestedName == "当前局域网")
    }

    @Test
    func missingEntitlementMapsToStableProductError() {
        let nativeError = NSError(
            domain: OSSystemExtensionError.errorDomain,
            code: OSSystemExtensionError.Code.missingEntitlement.rawValue
        )

        let error = BridgeError.from(nativeError)

        #expect(error.code == "NP_MAC_MISSING_ENTITLEMENT")
        #expect(error.message.contains("权限"))
    }

    @Test
    func hostStateJsonIncludesGatewayLifecycle() throws {
        let extensionSnapshot = SystemExtensionSnapshot(
            bundleIdentifier: "com.nonproxy.test",
            installed: true,
            enabled: true,
            awaitingUserApproval: false,
            uninstalling: false,
            bundleVersion: "1",
            bundleShortVersion: "0.1.0"
        )
        let preference = NetworkPreferenceSnapshot(
            configured: true,
            enabled: true
        )
        let state = MacHostState(
            gatewayAgent: GatewayAgentSnapshot(
                registered: true,
                enabled: true,
                requiresApproval: false,
                found: true,
                ready: true,
                requiresUpgrade: false
            ),
            transparentExtension: extensionSnapshot,
            dnsExtension: extensionSnapshot,
            transparentPreference: preference,
            dnsPreference: preference
        )

        let data = try JSONEncoder().encode(state)
        let json = String(decoding: data, as: UTF8.self)
        let decoded = try JSONDecoder().decode(
            MacHostState.self,
            from: data
        )

        #expect(json.contains(#""gatewayAgent""#))
        #expect(decoded == state)
    }

    @MainActor
    @Test
    func hostStateMessageDoesNotHideMissingPackageOrStalePreferences() {
        let absentExtension = SystemExtensionSnapshot(
            bundleIdentifier: "com.nonproxy.test",
            installed: false,
            enabled: false,
            awaitingUserApproval: false,
            uninstalling: false,
            bundleVersion: nil,
            bundleShortVersion: nil
        )
        let disabledPreference = NetworkPreferenceSnapshot(
            configured: false,
            enabled: false
        )
        let missingPackage = MacHostState(
            gatewayAgent: GatewayAgentSnapshot(
                registered: false,
                enabled: false,
                requiresApproval: false,
                found: false,
                ready: false,
                requiresUpgrade: false
            ),
            transparentExtension: absentExtension,
            dnsExtension: absentExtension,
            transparentPreference: disabledPreference,
            dnsPreference: disabledPreference
        )
        let stalePreferences = MacHostState(
            gatewayAgent: GatewayAgentSnapshot(
                registered: false,
                enabled: false,
                requiresApproval: false,
                found: true,
                ready: false,
                requiresUpgrade: false
            ),
            transparentExtension: absentExtension,
            dnsExtension: absentExtension,
            transparentPreference: NetworkPreferenceSnapshot(
                configured: true,
                enabled: false
            ),
            dnsPreference: disabledPreference
        )

        #expect(
            MacHostBridgeService.stateMessage(missingPackage)
                .contains("缺少")
        )
        #expect(
            MacHostBridgeService.stateMessage(stalePreferences)
                .contains("部分")
        )
    }
}

@Suite(.serialized)
struct BridgeOperationGateTests {
    @Test
    func permitsOnlyOneMutableOperation() {
        let gate = BridgeOperationGate.shared
        #expect(gate.begin())
        defer { gate.end() }

        #expect(!gate.begin())
    }

    @Test
    func permitsNextOperationAfterCompletion() {
        let gate = BridgeOperationGate.shared
        #expect(gate.begin())
        gate.end()

        #expect(gate.begin())
        gate.end()
    }
}
