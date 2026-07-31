import NonProxyProviderContracts
@testable import NonProxyProviderCore
import XCTest

private actor RecordingDecisionControl: ProviderDecisionReporting {
    private(set) var decisions: [Nonproxy_Provider_V1_DecisionRecord] = []
    private(set) var droppedEvents: UInt64 = 0
    private(set) var attemptedBatchIDs: [String] = []
    private var remainingFailures: Int

    init(remainingFailures: Int = 0) {
        self.remainingFailures = remainingFailures
    }

    func reportDecisionBatch(
        _ decisions: [Nonproxy_Provider_V1_DecisionRecord],
        batchID: String,
        droppedEvents: UInt64
    ) async throws {
        try await Task.sleep(for: .milliseconds(20))
        attemptedBatchIDs.append(batchID)
        if remainingFailures > 0 {
            remainingFailures -= 1
            throw ProviderError.control("测试瞬时失败")
        }
        self.decisions.append(contentsOf: decisions)
        self.droppedEvents += droppedEvents
    }

    func snapshot() -> (Int, UInt64, [String]) {
        (decisions.count, droppedEvents, attemptedBatchIDs)
    }
}

final class ProviderDecisionReporterTests: XCTestCase {
    func testBoundsQueueAndReportsDropCountWithAcceptedRecords() async {
        let control = RecordingDecisionControl()
        let reporter = ProviderDecisionReporter(
            control: control,
            capacity: 2,
            batchSize: 1,
            retryDelay: .milliseconds(5)
        )
        var first = Nonproxy_Provider_V1_DecisionRecord()
        first.context.flowID = "one"
        var second = Nonproxy_Provider_V1_DecisionRecord()
        second.context.flowID = "two"

        XCTAssertTrue(reporter.submit(first))
        XCTAssertTrue(reporter.submit(second))
        XCTAssertFalse(reporter.submit(Nonproxy_Provider_V1_DecisionRecord()))
        reporter.recordUnreportable()
        let completed = await waitUntil {
            let snapshot = await control.snapshot()
            return snapshot.0 == 2 && snapshot.1 == 2
        }
        reporter.stop()

        XCTAssertTrue(completed)
    }

    func testRetriesTheSameInFlightBatchAfterTransientFailure() async {
        let control = RecordingDecisionControl(remainingFailures: 1)
        let reporter = ProviderDecisionReporter(
            control: control,
            capacity: 2,
            batchSize: 2,
            retryDelay: .milliseconds(5)
        )
        var record = Nonproxy_Provider_V1_DecisionRecord()
        record.context.flowID = "retry-once"

        XCTAssertTrue(reporter.submit(record))
        let completed = await waitUntil {
            let snapshot = await control.snapshot()
            return snapshot.0 == 1
        }
        reporter.stop()

        XCTAssertTrue(completed)
        let snapshot = await control.snapshot()
        XCTAssertEqual(snapshot.0, 1)
        XCTAssertEqual(snapshot.2.count, 2)
        XCTAssertEqual(Set(snapshot.2).count, 1)
    }

    private func waitUntil(
        _ condition: @escaping @Sendable () async -> Bool
    ) async -> Bool {
        for _ in 0 ..< 100 {
            if await condition() {
                return true
            }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return false
    }
}
