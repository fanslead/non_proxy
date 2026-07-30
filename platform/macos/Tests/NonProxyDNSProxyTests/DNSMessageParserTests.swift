import Foundation
@testable import NonProxyDNSProxy
import XCTest

final class DNSMessageParserTests: XCTestCase {
    func testParsesOrdinaryQueryAndCanonicalizesName() throws {
        let query = makeQuery(name: ["WWW", "Example", "COM"], type: 1)
        let question = try DNSMessageParser.parseQuery(query)

        XCTAssertEqual(question.transactionID, 0x1234)
        XCTAssertEqual(question.name, "www.example.com")
        XCTAssertEqual(question.type, 1)
        XCTAssertEqual(question.queryClass, 1)
        XCTAssertEqual(question.questionEndOffset, query.count)
    }

    func testAcceptsServiceLabelsAndRootName() throws {
        let service = try DNSMessageParser.parseQuery(
            makeQuery(name: ["_dns-sd", "_udp", "local"], type: 12)
        )
        let root = try DNSMessageParser.parseQuery(
            makeQuery(name: [], type: 2)
        )

        XCTAssertEqual(service.name, "_dns-sd._udp.local")
        XCTAssertEqual(root.name, ".")
    }

    func testDecodesCompressedQuestionName() throws {
        var query = header()
        query.append(contentsOf: [0xC0, 0x12, 0, 1, 0, 1])
        query.append(contentsOf: encodedName(["example", "com"]))

        let question = try DNSMessageParser.parseQuery(query)

        XCTAssertEqual(question.name, "example.com")
        XCTAssertEqual(question.questionEndOffset, 18)
    }

    func testRejectsPointerLoopAndMultipleQuestions() {
        var loop = header()
        loop.append(contentsOf: [0xC0, 0x0C, 0, 1, 0, 1])
        XCTAssertThrowsError(try DNSMessageParser.parseQuery(loop))

        var multiple = makeQuery(name: ["example", "com"], type: 1)
        multiple[5] = 2
        XCTAssertThrowsError(try DNSMessageParser.parseQuery(multiple))
    }

    func testValidatesMatchingResponse() throws {
        let query = makeQuery(name: ["example", "com"], type: 1)
        let question = try DNSMessageParser.parseQuery(query)
        let response = DNSResponseBuilder.refused(
            query: query,
            question: question
        )

        XCTAssertNoThrow(
            try DNSMessageParser.validateResponse(response, for: question)
        )
        var mismatched = response
        mismatched[1] ^= 1
        XCTAssertThrowsError(
            try DNSMessageParser.validateResponse(mismatched, for: question)
        )
    }

    private func makeQuery(name: [String], type: UInt16) -> Data {
        var query = header()
        query.append(contentsOf: encodedName(name))
        query.append(UInt8((type >> 8) & 0xFF))
        query.append(UInt8(type & 0xFF))
        query.append(contentsOf: [0, 1])
        return query
    }

    private func header() -> Data {
        Data([
            0x12, 0x34,
            0x01, 0x00,
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x00,
            0x00, 0x00,
        ])
    }

    private func encodedName(_ labels: [String]) -> [UInt8] {
        labels.flatMap {
            [UInt8($0.utf8.count)] + Array($0.utf8)
        } + [0]
    }
}
