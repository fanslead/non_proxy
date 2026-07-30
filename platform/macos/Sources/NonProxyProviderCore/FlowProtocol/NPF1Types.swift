import Foundation

public enum NPF1ProtocolError: Error, Sendable, Equatable {
    case invalidMagic
    case unsupportedVersion
    case invalidFrameType
    case invalidFlags
    case invalidFlowID
    case invalidSequence
    case sequenceExhausted
    case payloadTooLarge
    case invalidPayload
}

public enum NPF1FrameType: UInt8, Sendable {
    case openTCP = 1
    case openUDP = 2
    case data = 3
    case datagram = 4
    case halfClose = 5
    case close = 6
    case windowUpdate = 7
    case error = 8
    case ping = 9
    case pong = 10

    var requiresEmptyPayload: Bool {
        switch self {
        case .halfClose, .close, .ping, .pong:
            true
        default:
            false
        }
    }
}

public struct NPF1FlowID: Sendable, Hashable {
    public static let byteCount = 16

    public let bytes: Data

    public init(bytes: Data) throws {
        guard bytes.count == Self.byteCount,
              bytes.contains(where: { $0 != 0 })
        else {
            throw NPF1ProtocolError.invalidFlowID
        }
        self.bytes = bytes
    }

    public static func random() throws -> Self {
        var generator = SystemRandomNumberGenerator()
        for _ in 0..<4 {
            let bytes = Data((0..<Self.byteCount).map { _ in
                UInt8.random(in: .min ... .max, using: &generator)
            })
            if let value = try? Self(bytes: bytes) {
                return value
            }
        }
        throw NPF1ProtocolError.invalidFlowID
    }
}

public struct NPF1Frame: Sendable, CustomDebugStringConvertible {
    public let type: NPF1FrameType
    public let flowID: NPF1FlowID
    public let sequence: UInt64
    public let payload: Data

    public init(
        type: NPF1FrameType,
        flowID: NPF1FlowID,
        sequence: UInt64,
        payload: Data = Data()
    ) throws {
        guard payload.count <= NPF1FrameCodec.maximumPayloadBytes else {
            throw NPF1ProtocolError.payloadTooLarge
        }
        guard !type.requiresEmptyPayload || payload.isEmpty else {
            throw NPF1ProtocolError.invalidPayload
        }
        self.type = type
        self.flowID = flowID
        self.sequence = sequence
        self.payload = payload
    }

    public var debugDescription: String {
        "NPF1Frame(type: \(type), flowID: \(flowID), "
            + "sequence: \(sequence), payloadBytes: \(payload.count))"
    }
}

public struct NPF1SequenceTracker: Sendable {
    private var expected: UInt64 = 0

    public init() {}

    public mutating func accept(_ sequence: UInt64) throws {
        guard sequence == expected else {
            throw NPF1ProtocolError.invalidSequence
        }
        guard expected < UInt64.max else {
            throw NPF1ProtocolError.sequenceExhausted
        }
        expected += 1
    }

    public var expectedSequence: UInt64 {
        expected
    }
}
