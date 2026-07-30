import Foundation
import NonProxyNativeMessaging
import XCTest

final class NativeMessageCodecTests: XCTestCase {
    func testDecodesDomainOnlyObservationWithoutURLFields() throws {
        let data = Data(
            """
            {
              "protocolVersion": 1,
              "requestID": "request-1",
              "type": "observeLearning",
              "payload": {
                "sessionID": "learning-1",
                "observationID": "observation-1",
                "browserContextID": "context-1",
                "kind": "subresource",
                "normalizedDomain": "api.example.com",
                "initiatorDomain": "example.com",
                "resourceType": "fetch"
              }
            }
            """.utf8
        )

        let request = try NativeMessageCodec().decode(data)

        guard case .observe(let payload) = request.payload else {
            return XCTFail("消息类型应为学习观测")
        }
        XCTAssertEqual(payload.normalizedDomain, "api.example.com")
        XCTAssertEqual(payload.initiatorDomain, "example.com")
    }

    func testRejectsUnsupportedVersionAndUnsafeRequestID() {
        let unsupported = message(version: 2, requestID: "request-1")
        let unsafe = message(version: 1, requestID: "../request")

        XCTAssertThrowsError(
            try NativeMessageCodec().decode(unsupported)
        )
        XCTAssertThrowsError(
            try NativeMessageCodec().decode(unsafe)
        )
    }

    func testDecodesCandidateConfirmationWithoutURLData() throws {
        let data = Data(
            """
            {
              "protocolVersion": 1,
              "requestID": "request-confirm",
              "type": "confirmLearning",
              "payload": {
                "sessionID": "learning-1",
                "confirmationID": "confirmation-1",
                "selectedDomains": [
                  "example.com",
                  "api.example.com"
                ]
              }
            }
            """.utf8
        )

        let request = try NativeMessageCodec().decode(data)

        guard case .confirm(let payload) = request.payload else {
            return XCTFail("消息类型应为候选确认")
        }
        XCTAssertEqual(
            payload.selectedDomains,
            ["example.com", "api.example.com"]
        )
    }

    func testEncodesStableErrorEnvelope() throws {
        let response = NativeResponse.failure(
            requestID: "request-1",
            error: .invalidCaller
        )
        let data = try NativeMessageCodec().encode(response)
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data)
                as? [String: Any]
        )

        XCTAssertEqual(object["protocolVersion"] as? Int, 1)
        XCTAssertEqual(object["requestID"] as? String, "request-1")
        XCTAssertEqual(object["ok"] as? Bool, false)
        let error = try XCTUnwrap(object["error"] as? [String: Any])
        XCTAssertEqual(
            error["code"] as? String,
            "NP_NATIVE_CALLER_INVALID"
        )
    }

    private func message(
        version: Int,
        requestID: String
    ) -> Data {
        Data(
            """
            {
              "protocolVersion": \(version),
              "requestID": "\(requestID)",
              "type": "hello",
              "payload": {}
            }
            """.utf8
        )
    }
}
