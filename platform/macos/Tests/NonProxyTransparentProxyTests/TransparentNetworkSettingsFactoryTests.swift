@testable import NonProxyTransparentProxy
import XCTest

final class TransparentNetworkSettingsFactoryTests: XCTestCase {
    func testCapturesOutboundTcpAndUdpWithoutLoopbackRule() throws {
        let settings = TransparentNetworkSettingsFactory.make()
        let rule = try XCTUnwrap(settings.includedNetworkRules?.only)

        XCTAssertEqual(rule.matchProtocol, .any)
        XCTAssertEqual(rule.matchDirection, .outbound)
        XCTAssertNil(rule.matchRemoteHostOrNetworkEndpoint)
        XCTAssertNil(rule.matchLocalNetworkEndpoint)
        XCTAssertEqual(settings.excludedNetworkRules?.count, 0)
    }
}

private extension Array {
    var only: Element? {
        count == 1 ? first : nil
    }
}
