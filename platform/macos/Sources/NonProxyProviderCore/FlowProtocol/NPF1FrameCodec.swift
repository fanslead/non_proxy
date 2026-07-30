import Foundation

public enum NPF1FrameCodec {
    public static let magic = Data("NPF1".utf8)
    public static let version: UInt16 = 1
    public static let headerBytes = 36
    public static let maximumPayloadBytes = 256 * 1024

    public static func encode(_ frame: NPF1Frame) throws -> Data {
        guard frame.payload.count <= maximumPayloadBytes,
              let payloadLength = UInt32(exactly: frame.payload.count)
        else {
            throw NPF1ProtocolError.payloadTooLarge
        }
        var output = Data(capacity: headerBytes + frame.payload.count)
        output.append(magic)
        output.appendBigEndian(version)
        output.append(frame.type.rawValue)
        output.append(0)
        output.append(frame.flowID.bytes)
        output.appendBigEndian(frame.sequence)
        output.appendBigEndian(payloadLength)
        output.append(frame.payload)
        return output
    }
}

public struct NPF1FrameDecoder: Sendable {
    private static let maximumBufferedBytes =
        NPF1FrameCodec.headerBytes + NPF1FrameCodec.maximumPayloadBytes

    private var buffer = Data()

    public init() {}

    public mutating func append(_ data: Data) throws -> [NPF1Frame] {
        guard !data.isEmpty else {
            return []
        }
        guard buffer.count <= Self.maximumBufferedBytes - data.count else {
            throw NPF1ProtocolError.payloadTooLarge
        }
        buffer.append(data)
        var frames: [NPF1Frame] = []
        while let frame = try decodeFirst() {
            frames.append(frame)
        }
        return frames
    }

    private mutating func decodeFirst() throws -> NPF1Frame? {
        guard buffer.count >= NPF1FrameCodec.headerBytes else {
            return nil
        }
        guard buffer.prefix(4) == NPF1FrameCodec.magic else {
            throw NPF1ProtocolError.invalidMagic
        }
        guard try buffer.readUInt16(at: 4) == NPF1FrameCodec.version else {
            throw NPF1ProtocolError.unsupportedVersion
        }
        guard let type = NPF1FrameType(rawValue: buffer[6]) else {
            throw NPF1ProtocolError.invalidFrameType
        }
        guard buffer[7] == 0 else {
            throw NPF1ProtocolError.invalidFlags
        }
        let flowID = try NPF1FlowID(bytes: buffer.subdata(in: 8..<24))
        let sequence = try buffer.readUInt64(at: 24)
        let payloadLength = Int(try buffer.readUInt32(at: 32))
        guard payloadLength <= NPF1FrameCodec.maximumPayloadBytes else {
            throw NPF1ProtocolError.payloadTooLarge
        }
        let frameLength = NPF1FrameCodec.headerBytes + payloadLength
        guard buffer.count >= frameLength else {
            return nil
        }
        let payload = buffer.subdata(
            in: NPF1FrameCodec.headerBytes..<frameLength
        )
        let frame = try NPF1Frame(
            type: type,
            flowID: flowID,
            sequence: sequence,
            payload: payload
        )
        // Data.removeFirst 会保留非零 startIndex，重新物化以维持协议偏移从 0 计数。
        buffer = Data(buffer.dropFirst(frameLength))
        return frame
    }
}

extension Data {
    mutating func appendBigEndian<T: FixedWidthInteger>(_ value: T) {
        for shift in stride(
            from: (T.bitWidth - 8),
            through: 0,
            by: -8
        ) {
            append(UInt8(truncatingIfNeeded: value >> T(shift)))
        }
    }

    func readUInt16(at offset: Int) throws -> UInt16 {
        try readInteger(at: offset, byteCount: 2)
    }

    func readUInt32(at offset: Int) throws -> UInt32 {
        try readInteger(at: offset, byteCount: 4)
    }

    func readUInt64(at offset: Int) throws -> UInt64 {
        try readInteger(at: offset, byteCount: 8)
    }

    private func readInteger<T: FixedWidthInteger>(
        at offset: Int,
        byteCount: Int
    ) throws -> T {
        guard offset >= 0,
              byteCount > 0,
              offset <= count - byteCount
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        var value: T = 0
        for byte in self[offset..<(offset + byteCount)] {
            value = (value << 8) | T(byte)
        }
        return value
    }
}
