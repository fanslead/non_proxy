import Foundation
@testable import NonProxyTransparentProxy
import XCTest

final class TransparentProviderStateTests: XCTestCase {
    func testRejectsConcurrentStartAndAllowsRetryAfterFailure() throws {
        let state = TransparentProviderState()
        let firstRun = try state.beginStart()

        XCTAssertThrowsError(try state.beginStart())

        state.failStart(runID: firstRun)
        XCTAssertNoThrow(try state.beginStart())
    }

    func testStopInvalidatesAnInFlightStart() throws {
        let state = TransparentProviderState()
        let runID = try state.beginStart()

        XCTAssertNil(state.remove())
        XCTAssertFalse(state.isCurrentStart(runID: runID))
    }
}
