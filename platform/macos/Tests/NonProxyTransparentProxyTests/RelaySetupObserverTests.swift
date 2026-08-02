@testable import NonProxyTransparentProxy
import Synchronization
import XCTest

final class RelaySetupObserverTests: XCTestCase {
    func testReportsOnlyTheFirstSetupOutcome() {
        let establishedCount = Mutex(0)
        let failures = Mutex<[String]>([])
        let observer = RelaySetupObserver(
            onEstablished: { selectedOutboundID in
                XCTAssertEqual(selectedOutboundID, "primary")
                establishedCount.withLock { $0 += 1 }
            },
            onFailed: { code in
                failures.withLock { $0.append(code) }
            }
        )

        observer.failed(code: "NP_PROXY_CONNECT_FAILED")
        observer.established(selectedOutboundID: "primary")
        observer.failed(code: "NP_PROXY_SECOND_FAILURE")

        XCTAssertEqual(establishedCount.withLock { $0 }, 0)
        XCTAssertEqual(
            failures.withLock { $0 },
            ["NP_PROXY_CONNECT_FAILED"]
        )
    }
}
