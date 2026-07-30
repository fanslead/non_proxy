import NonProxyNativeMessaging
import XCTest

final class BrowserCallerTests: XCTestCase {
    func testAcceptsOnlyThePinnedChromiumExtensionOrigin() throws {
        let allowed =
            "chrome-extension://\(BrowserCaller.chromiumExtensionID)/"
        XCTAssertEqual(
            try BrowserCaller(arguments: ["host", allowed]).origin,
            allowed
        )
        XCTAssertThrowsError(
            try BrowserCaller(
                arguments: ["host", "chrome-extension://attacker/"]
            )
        )
        XCTAssertThrowsError(
            try BrowserCaller(arguments: ["host"])
        )
    }
}
