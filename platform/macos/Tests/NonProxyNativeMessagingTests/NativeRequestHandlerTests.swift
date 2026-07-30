import NonProxyNativeMessaging
import XCTest

final class NativeRequestHandlerTests: XCTestCase {
    func testRoutesLearningRequestAndKeepsStableRequestID() async {
        let service = LearningServiceStub()
        let handler = NativeRequestHandler(service: service)
        let request = NativeRequest(
            requestID: "request-1",
            payload: .start(
                StartLearningPayload(
                    normalizedSite: "example.com",
                    browserContextID: "context-1",
                    durationMilliseconds: 60_000
                )
            )
        )

        let response = await handler.handle(request)

        XCTAssertTrue(response.ok)
        XCTAssertEqual(response.requestID, "request-1")
        guard case .started(let result) = response.payload else {
            return XCTFail("应返回学习会话")
        }
        XCTAssertEqual(result.sessionID, "learning-test")
        let startedSites = await service.startedSites()
        XCTAssertEqual(
            startedSites,
            ["example.com"]
        )
    }
}

private actor LearningServiceStub: NativeLearningServing {
    private var sites: [String] = []

    func start(
        _ payload: StartLearningPayload
    ) async throws -> StartLearningResult {
        sites.append(payload.normalizedSite)
        return StartLearningResult(
            sessionID: "learning-test",
            expiresAtUnixMilliseconds: 61_000
        )
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
        ConfirmLearningResult(
            policies: [],
            snapshotVersion: 1,
            snapshotState: "pendingAck",
            replayed: false
        )
    }

    nonisolated func shutdown() {}

    func startedSites() -> [String] {
        sites
    }
}
