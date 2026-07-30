import Foundation
import NonProxyNativeMessaging
import XCTest

final class NativeRequestProcessorTests: XCTestCase {
    func testProcessesPropertyListCompatibleJSONRequest() async throws {
        let processor = NativeRequestProcessor(
            handler: NativeRequestHandler(
                service: ProcessorLearningServiceStub()
            )
        )
        let request = Data(
            """
            {
              "protocolVersion": 1,
              "requestID": "safari-1",
              "type": "hello",
              "payload": {}
            }
            """.utf8
        )

        let responseData = try await processor.process(request)
        let response = try XCTUnwrap(
            JSONSerialization.jsonObject(with: responseData)
                as? [String: Any]
        )

        XCTAssertEqual(response["requestID"] as? String, "safari-1")
        XCTAssertEqual(response["ok"] as? Bool, true)
    }

    func testRuntimeFailureKeepsValidRequestIdentity() throws {
        let processor = NativeRequestProcessor(
            handler: NativeRequestHandler(
                service: ProcessorLearningServiceStub()
            )
        )
        let request = Data(
            """
            {
              "protocolVersion": 1,
              "requestID": "safari-2",
              "type": "hello",
              "payload": {}
            }
            """.utf8
        )

        let responseData = processor.failure(
            for: request,
            error: .runtimeUnavailable("测试不可用")
        )
        let response = try XCTUnwrap(
            JSONSerialization.jsonObject(with: responseData)
                as? [String: Any]
        )

        XCTAssertEqual(response["requestID"] as? String, "safari-2")
        XCTAssertEqual(response["ok"] as? Bool, false)
        let error = try XCTUnwrap(response["error"] as? [String: Any])
        XCTAssertEqual(
            error["code"] as? String,
            "NP_NATIVE_RUNTIME_UNAVAILABLE"
        )
    }
}

private struct ProcessorLearningServiceStub: NativeLearningServing {
    func start(
        _ payload: StartLearningPayload
    ) async throws -> StartLearningResult {
        throw NativeMessagingError.invalidMessage("测试未实现")
    }

    func observe(
        _ payload: ObserveLearningPayload
    ) async throws -> ObservationResult {
        throw NativeMessagingError.invalidMessage("测试未实现")
    }

    func list(
        _ payload: SessionPayload
    ) async throws -> CandidateListResult {
        throw NativeMessagingError.invalidMessage("测试未实现")
    }

    func stop(
        _ payload: SessionPayload
    ) async throws -> StopLearningResult {
        throw NativeMessagingError.invalidMessage("测试未实现")
    }

    func confirm(
        _ payload: ConfirmLearningPayload
    ) async throws -> ConfirmLearningResult {
        throw NativeMessagingError.invalidMessage("测试未实现")
    }

    func shutdown() {}
}
