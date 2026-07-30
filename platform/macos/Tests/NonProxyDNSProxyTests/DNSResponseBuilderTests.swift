import Foundation
@testable import NonProxyDNSProxy
import XCTest

final class DNSResponseBuilderTests: XCTestCase {
    func testBuildsRefusedResponseWithoutAdditionalRecords() throws {
        var query = makeQuery()
        query[11] = 1
        query.append(contentsOf: [0, 0, 41, 16, 0, 0, 0, 0, 0, 0, 0])
        let question = try DNSMessageParser.parseQuery(query)

        let response = DNSResponseBuilder.refused(
            query: query,
            question: question
        )

        XCTAssertEqual(readUInt16(response, at: 0), 0xCAFE)
        XCTAssertEqual(readUInt16(response, at: 2) & 0x800F, 0x8005)
        XCTAssertEqual(readUInt16(response, at: 4), 1)
        XCTAssertEqual(readUInt16(response, at: 6), 0)
        XCTAssertEqual(readUInt16(response, at: 8), 0)
        XCTAssertEqual(readUInt16(response, at: 10), 0)
        XCTAssertEqual(response.count, question.questionEndOffset)
    }

    func testBuildsFormatErrorForMalformedQuestion() {
        let response = DNSResponseBuilder.formatError(query: Data([
            0xBE, 0xEF, 0x01, 0x00,
            0, 2, 0, 0, 0, 0, 0, 0,
        ]))

        XCTAssertEqual(readUInt16(response, at: 0), 0xBEEF)
        XCTAssertEqual(readUInt16(response, at: 2) & 0x800F, 0x8001)
        XCTAssertEqual(readUInt16(response, at: 4), 0)
    }

    private func makeQuery() -> Data {
        Data([
            0xCA, 0xFE, 0x01, 0x00,
            0, 1, 0, 0, 0, 0, 0, 0,
            7, 101, 120, 97, 109, 112, 108, 101,
            3, 99, 111, 109, 0,
            0, 1, 0, 1,
        ])
    }

    private func readUInt16(_ data: Data, at offset: Int) -> UInt16 {
        UInt16(data[offset]) << 8 | UInt16(data[offset + 1])
    }
}
