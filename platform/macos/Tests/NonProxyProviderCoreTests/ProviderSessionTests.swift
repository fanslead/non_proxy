import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import SwiftProtobuf
import XCTest

final class ProviderSessionTests: XCTestCase {
    func testIssuesStrictlyIncreasingRequestSequences() async throws {
        let session = ProviderSession(instanceID: "instance")
        try await session.install(response: registrationResponse(expiresIn: 60))

        let first = try await session.requestContext()
        let second = try await session.requestContext()

        XCTAssertEqual(first.requestSequence, 1)
        XCTAssertEqual(second.requestSequence, 2)
        XCTAssertEqual(first.providerInstanceID, "instance")
    }

    func testRejectsExpiredSession() async throws {
        let session = ProviderSession(instanceID: "instance")
        try await session.install(response: registrationResponse(expiresIn: -1))

        do {
            _ = try await session.requestContext()
            XCTFail("过期会话不应生成请求上下文")
        } catch let error as ProviderError {
            XCTAssertEqual(error.code, "NP_PROVIDER_SESSION_INVALID")
        }
    }

    private func registrationResponse(
        expiresIn seconds: TimeInterval
    ) -> Nonproxy_Provider_V1_RegisterProviderResponse {
        let expiration = Date().addingTimeInterval(seconds)
        var timestamp = Google_Protobuf_Timestamp()
        timestamp.seconds = Int64(expiration.timeIntervalSince1970)

        var response = Nonproxy_Provider_V1_RegisterProviderResponse()
        response.accepted = true
        response.sessionToken = Data(repeating: 7, count: 32)
        response.sessionExpiresAt = timestamp
        response.providerGeneration = 1
        return response
    }
}
