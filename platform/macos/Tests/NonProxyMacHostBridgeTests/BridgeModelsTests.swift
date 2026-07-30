import Foundation
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
    func missingEntitlementMapsToStableProductError() {
        let nativeError = NSError(
            domain: OSSystemExtensionError.errorDomain,
            code: OSSystemExtensionError.Code.missingEntitlement.rawValue
        )

        let error = BridgeError.from(nativeError)

        #expect(error.code == "NP_MAC_MISSING_ENTITLEMENT")
        #expect(error.message.contains("权限"))
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
