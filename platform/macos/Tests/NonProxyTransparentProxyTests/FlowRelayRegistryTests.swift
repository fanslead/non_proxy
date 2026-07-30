import Foundation
@testable import NonProxyTransparentProxy
import Synchronization
import XCTest

final class FlowRelayRegistryTests: XCTestCase {
    func testBoundsActiveRelaysAndCancelsRegisteredRelays() {
        let registry = FlowRelayRegistry()
        let first = TestFlowRelay()
        registry.beginAccepting()

        XCTAssertTrue(registry.insert(first))
        for _ in 1..<2_048 {
            XCTAssertTrue(registry.insert(TestFlowRelay()))
        }

        XCTAssertFalse(registry.insert(TestFlowRelay()))
        XCTAssertEqual(registry.activeFlowCount, 2_048)

        registry.stopAcceptingAndCancelAll()

        XCTAssertEqual(registry.activeFlowCount, 0)
        XCTAssertEqual(first.cancelCount, 1)
    }

    func testBoundsAndReleasesGlobalQueuedBytes() {
        let registry = FlowRelayRegistry()
        let capacity = 32 * 1024 * 1024
        registry.beginAccepting()

        XCTAssertTrue(registry.reserve(bytes: capacity))
        XCTAssertEqual(registry.queuedBytes, UInt64(capacity))
        XCTAssertFalse(registry.reserve(bytes: 1))

        registry.release(bytes: capacity)

        XCTAssertEqual(registry.queuedBytes, 0)
        XCTAssertTrue(registry.reserve(bytes: 1))
    }

    func testRejectsInvalidReservationsWithoutChangingUsage() {
        let registry = FlowRelayRegistry()
        registry.beginAccepting()

        XCTAssertFalse(registry.reserve(bytes: -1))
        XCTAssertFalse(registry.reserve(bytes: Int.max))
        registry.release(bytes: -1)

        XCTAssertEqual(registry.queuedBytes, 0)
    }

    func testStopAtomicallyRejectsNewRelaysAndReservations() {
        let registry = FlowRelayRegistry()
        registry.beginAccepting()
        XCTAssertTrue(registry.insert(TestFlowRelay()))
        XCTAssertTrue(registry.reserve(bytes: 1))

        registry.stopAcceptingAndCancelAll()

        XCTAssertFalse(registry.insert(TestFlowRelay()))
        XCTAssertFalse(registry.reserve(bytes: 1))
        registry.release(bytes: 1)
        XCTAssertEqual(registry.queuedBytes, 0)
    }
}

private final class TestFlowRelay: FlowRelay, Sendable {
    private let count = Mutex(0)

    var cancelCount: Int {
        count.withLock { $0 }
    }

    func start() {}

    func cancel() {
        count.withLock { $0 += 1 }
    }
}
