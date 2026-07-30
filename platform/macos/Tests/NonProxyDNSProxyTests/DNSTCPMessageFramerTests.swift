import Foundation
@testable import NonProxyDNSProxy
import XCTest

final class DNSTCPMessageFramerTests: XCTestCase {
    func testAcceptsFragmentedAndCoalescedFrames() throws {
        let first = Data([1, 2, 3])
        let second = Data([4, 5])
        let combined = try DNSTCPMessageFramer.frame(first)
            + DNSTCPMessageFramer.frame(second)
        var framer = DNSTCPMessageFramer()

        XCTAssertEqual(
            try framer.append(combined.prefix(4)),
            []
        )
        XCTAssertEqual(
            try framer.append(combined.dropFirst(4)),
            [first, second]
        )
    }

    func testRejectsZeroLengthFrame() {
        var framer = DNSTCPMessageFramer()
        XCTAssertThrowsError(try framer.append(Data([0, 0])))
    }
}
