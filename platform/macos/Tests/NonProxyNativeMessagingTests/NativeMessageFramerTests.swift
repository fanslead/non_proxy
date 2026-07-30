import Foundation
import NonProxyNativeMessaging
import XCTest

final class NativeMessageFramerTests: XCTestCase {
    func testFramesLittleEndianPayloadAndReadsItBack() throws {
        let framer = NativeMessageFramer()
        let payload = Data(#"{"message":"你好"}"#.utf8)
        let framed = try framer.frame(payload)

        XCTAssertEqual(Array(framed.prefix(4)), [20, 0, 0, 0])

        let input = Pipe()
        try input.fileHandleForWriting.write(contentsOf: framed)
        try input.fileHandleForWriting.close()
        XCTAssertEqual(
            try framer.readMessage(from: input.fileHandleForReading),
            payload
        )
        XCTAssertNil(
            try framer.readMessage(from: input.fileHandleForReading)
        )
    }

    func testRejectsZeroPartialAndOversizedFrames() throws {
        let framer = NativeMessageFramer()
        XCTAssertThrowsError(try read(Data([0, 0, 0, 0]), using: framer))
        XCTAssertThrowsError(try read(Data([2, 0, 0, 0, 1]), using: framer))

        let oversized = UInt32(
            NativeMessageFramer.maximumInputBytes + 1
        )
        let header = Data([
            UInt8(oversized & 0xff),
            UInt8((oversized >> 8) & 0xff),
            UInt8((oversized >> 16) & 0xff),
            UInt8((oversized >> 24) & 0xff),
        ])
        XCTAssertThrowsError(try read(header, using: framer))
    }

    private func read(
        _ data: Data,
        using framer: NativeMessageFramer
    ) throws -> Data? {
        let pipe = Pipe()
        try pipe.fileHandleForWriting.write(contentsOf: data)
        try pipe.fileHandleForWriting.close()
        return try framer.readMessage(from: pipe.fileHandleForReading)
    }
}
