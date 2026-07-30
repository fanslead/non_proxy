import Foundation

public struct DNSTCPMessageFramer: Sendable {
    private var buffer = Data()
    private let maximumMessageLength: Int

    public init(maximumMessageLength: Int = 65_535) {
        self.maximumMessageLength = maximumMessageLength
    }

    public mutating func append(_ data: Data) throws -> [Data] {
        guard !data.isEmpty else {
            return []
        }
        guard data.count <= 1_048_576 else {
            throw DNSProxyError.invalidMessage("DNS TCP 读取批次过大")
        }
        buffer.append(data)
        var messages: [Data] = []
        while buffer.count >= 2 {
            let length = Int(buffer[0]) << 8 | Int(buffer[1])
            guard length > 0, length <= maximumMessageLength else {
                throw DNSProxyError.invalidMessage("DNS TCP 帧长度无效")
            }
            guard buffer.count >= length + 2 else {
                break
            }
            messages.append(buffer.subdata(in: 2 ..< length + 2))
            buffer.removeSubrange(0 ..< length + 2)
        }
        guard buffer.count <= maximumMessageLength + 2 else {
            throw DNSProxyError.invalidMessage("DNS TCP 缓冲区过大")
        }
        return messages
    }

    public static func frame(_ message: Data) throws -> Data {
        guard !message.isEmpty, message.count <= 65_535 else {
            throw DNSProxyError.invalidMessage("DNS TCP 响应长度无效")
        }
        let length = UInt16(message.count)
        var framed = Data([
            UInt8((length >> 8) & 0xFF),
            UInt8(length & 0xFF),
        ])
        framed.append(message)
        return framed
    }
}
