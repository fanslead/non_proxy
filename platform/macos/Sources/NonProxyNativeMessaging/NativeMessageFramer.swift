import Foundation

public struct NativeMessageFramer: Sendable {
    public static let maximumInputBytes = 128 * 1_024
    public static let maximumOutputBytes = 1_024 * 1_024

    public init() {}

    public func readMessage(
        from input: FileHandle
    ) throws -> Data? {
        guard let header = try readExactly(4, from: input) else {
            return nil
        }
        let bytes = [UInt8](header)
        let length = UInt32(bytes[0])
            | UInt32(bytes[1]) << 8
            | UInt32(bytes[2]) << 16
            | UInt32(bytes[3]) << 24
        guard length > 0 else {
            throw NativeMessagingError.invalidFrame
        }
        guard length <= Self.maximumInputBytes else {
            throw NativeMessagingError.messageTooLarge
        }
        guard let payload = try readExactly(Int(length), from: input) else {
            throw NativeMessagingError.invalidFrame
        }
        return payload
    }

    public func writeMessage(
        _ payload: Data,
        to output: FileHandle
    ) throws {
        try output.write(contentsOf: frame(payload))
    }

    public func frame(_ payload: Data) throws -> Data {
        guard !payload.isEmpty else {
            throw NativeMessagingError.invalidFrame
        }
        guard payload.count <= Self.maximumOutputBytes,
              let length = UInt32(exactly: payload.count)
        else {
            throw NativeMessagingError.messageTooLarge
        }
        let header = Data([
            UInt8(length & 0xff),
            UInt8((length >> 8) & 0xff),
            UInt8((length >> 16) & 0xff),
            UInt8((length >> 24) & 0xff),
        ])
        var framed = Data(capacity: header.count + payload.count)
        framed.append(header)
        framed.append(payload)
        return framed
    }

    private func readExactly(
        _ count: Int,
        from input: FileHandle
    ) throws -> Data? {
        var result = Data()
        while result.count < count {
            let chunk = try input.read(
                upToCount: count - result.count
            ) ?? Data()
            if chunk.isEmpty {
                if result.isEmpty {
                    return nil
                }
                throw NativeMessagingError.invalidFrame
            }
            result.append(chunk)
        }
        return result
    }
}
