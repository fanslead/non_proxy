import Foundation
@testable import NonProxyProviderCore
import XCTest

final class NPF1CodecTests: XCTestCase {
    func testFrameHeaderMatchesRustGoldenBytes() throws {
        let flowID = try NPF1FlowID(bytes: Data(repeating: 1, count: 16))
        let frame = try NPF1Frame(
            type: .openTCP,
            flowID: flowID,
            sequence: 0,
            payload: Data([7, 8, 9])
        )

        let encoded = try NPF1FrameCodec.encode(frame)

        var expected = Data("NPF1".utf8)
        expected.append(contentsOf: [0, 1, 1, 0])
        expected.append(Data(repeating: 1, count: 16))
        expected.append(Data(repeating: 0, count: 8))
        expected.append(contentsOf: [0, 0, 0, 3, 7, 8, 9])
        XCTAssertEqual(encoded, expected)
        XCTAssertFalse(frame.debugDescription.contains("7, 8, 9"))
    }

    func testDecoderHandlesFragmentedAndCombinedFrames() throws {
        let flowID = try NPF1FlowID(bytes: Data(repeating: 2, count: 16))
        let first = try NPF1FrameCodec.encode(
            NPF1Frame(
                type: .windowUpdate,
                flowID: flowID,
                sequence: 0,
                payload: try NPF1PayloadCodec.encodeWindowUpdate(65_536)
            )
        )
        let second = try NPF1FrameCodec.encode(
            NPF1Frame(
                type: .data,
                flowID: flowID,
                sequence: 1,
                payload: Data("hello".utf8)
            )
        )
        let combined = first + second
        var decoder = NPF1FrameDecoder()

        XCTAssertTrue(
            try decoder.append(combined.prefix(17)).isEmpty
        )
        let frames = try decoder.append(combined.dropFirst(17))

        XCTAssertEqual(frames.map(\.type), [.windowUpdate, .data])
        XCTAssertEqual(frames[1].payload, Data("hello".utf8))
    }

    func testOpenAndDatagramPayloadMatchWireContract() throws {
        let endpoint = try NPF1Endpoint(
            host: "Proxy.Example.com.",
            port: 443
        )
        let open = try NPF1PayloadCodec.encodeOpen(
            capability: Data(repeating: 0xAB, count: 32),
            outboundID: "primary",
            endpoint: endpoint,
            initialWindowBytes: 65_536
        )

        var expectedOpen = Data(repeating: 0xAB, count: 32)
        expectedOpen.append(7)
        expectedOpen.append(Data("primary".utf8))
        expectedOpen.append(contentsOf: [3, 17])
        expectedOpen.append(Data("proxy.example.com".utf8))
        expectedOpen.append(contentsOf: [1, 187, 0, 1, 0, 0])
        XCTAssertEqual(open, expectedOpen)
        let datagram = try NPF1PayloadCodec.encodeDatagram(
            endpoint: endpoint,
            content: Data([0x12, 0x34])
        )
        let decoded = try NPF1PayloadCodec.decodeDatagram(datagram)
        XCTAssertEqual(decoded.endpoint, endpoint)
        XCTAssertEqual(decoded.content, Data([0x12, 0x34]))
    }

    func testRejectsMalformedSequenceWindowAndOversizedDatagram() throws {
        var tracker = NPF1SequenceTracker()
        try tracker.accept(0)
        XCTAssertThrowsError(try tracker.accept(0))
        XCTAssertThrowsError(
            try NPF1PayloadCodec.decodeWindowUpdate(Data(repeating: 0, count: 4))
        )
        XCTAssertThrowsError(
            try NPF1PayloadCodec.encodeDatagram(
                endpoint: NPF1Endpoint(host: "dns.example", port: 53),
                content: Data(repeating: 1, count: 65_001)
            )
        )
    }

    func testDecoderRejectsInvalidMagicVersionAndFlags() throws {
        let flowID = try NPF1FlowID(bytes: Data(repeating: 3, count: 16))
        let valid = try NPF1FrameCodec.encode(
            NPF1Frame(type: .ping, flowID: flowID, sequence: 0)
        )

        for (offset, value) in [(0, UInt8(0)), (5, UInt8(2)), (7, UInt8(1))] {
            var malformed = valid
            malformed[offset] = value
            var decoder = NPF1FrameDecoder()
            XCTAssertThrowsError(try decoder.append(malformed))
        }
    }
}
