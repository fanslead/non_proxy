import Foundation

public enum DNSMessageParser {
    private static let headerLength = 12
    private static let maximumMessageLength = 65_535

    public static func parseQuery(_ message: Data) throws -> DNSQuestion {
        let bytes = [UInt8](message)
        guard bytes.count >= headerLength,
              bytes.count <= maximumMessageLength
        else {
            throw DNSProxyError.invalidMessage("DNS 查询长度无效")
        }
        let flags = readUInt16(bytes, at: 2)
        guard flags & 0x8000 == 0, flags & 0x7800 == 0 else {
            throw DNSProxyError.unsupportedQuery("DNS 消息不是标准查询")
        }
        guard flags & 0x000F == 0 else {
            throw DNSProxyError.invalidMessage("DNS 查询携带了响应码")
        }
        guard readUInt16(bytes, at: 4) == 1,
              readUInt16(bytes, at: 6) == 0,
              readUInt16(bytes, at: 8) == 0
        else {
            throw DNSProxyError.unsupportedQuery(
                "DNS 查询必须只包含一个问题且不能包含回答"
            )
        }
        return try parseQuestion(bytes, flags: flags)
    }

    public static func validateResponse(
        _ response: Data,
        for query: DNSQuestion
    ) throws {
        let bytes = [UInt8](response)
        guard bytes.count >= headerLength,
              bytes.count <= maximumMessageLength
        else {
            throw DNSProxyError.responseInvalid("DNS 响应长度无效")
        }
        let flags = readUInt16(bytes, at: 2)
        guard flags & 0x8000 != 0,
              flags & 0x7800 == query.flags & 0x7800,
              readUInt16(bytes, at: 4) == 1
        else {
            throw DNSProxyError.responseInvalid("DNS 响应头与查询不匹配")
        }
        let responseQuestion: DNSQuestion
        do {
            responseQuestion = try parseQuestion(bytes, flags: flags)
        } catch {
            throw DNSProxyError.responseInvalid("DNS 响应问题区无效")
        }
        guard responseQuestion.transactionID == query.transactionID,
              responseQuestion.name == query.name,
              responseQuestion.type == query.type,
              responseQuestion.queryClass == query.queryClass
        else {
            throw DNSProxyError.responseInvalid("DNS 响应问题与查询不匹配")
        }
    }

    private static func parseQuestion(
        _ bytes: [UInt8],
        flags: UInt16
    ) throws -> DNSQuestion {
        let decoded = try decodeName(bytes, start: headerLength)
        guard decoded.endOffset + 4 <= bytes.count else {
            throw DNSProxyError.invalidMessage("DNS 问题区不完整")
        }
        let type = readUInt16(bytes, at: decoded.endOffset)
        let queryClass = readUInt16(bytes, at: decoded.endOffset + 2)
        guard queryClass == 1 else {
            throw DNSProxyError.unsupportedQuery("只支持 IN 类型 DNS 查询")
        }
        return DNSQuestion(
            transactionID: readUInt16(bytes, at: 0),
            flags: flags,
            name: decoded.name,
            type: type,
            queryClass: queryClass,
            questionEndOffset: decoded.endOffset + 4
        )
    }

    private static func decodeName(
        _ bytes: [UInt8],
        start: Int
    ) throws -> (name: String, endOffset: Int) {
        var labels: [String] = []
        var position = start
        var endOffset: Int?
        var visited: Set<Int> = []
        var expandedLength = 1
        var pointerCount = 0

        while true {
            guard position < bytes.count else {
                throw DNSProxyError.invalidMessage("DNS 名称越界")
            }
            let length = Int(bytes[position])
            if length & 0xC0 == 0xC0 {
                guard position + 1 < bytes.count else {
                    throw DNSProxyError.invalidMessage("DNS 压缩指针不完整")
                }
                let target = ((length & 0x3F) << 8)
                    | Int(bytes[position + 1])
                guard target < bytes.count,
                      visited.insert(target).inserted
                else {
                    throw DNSProxyError.invalidMessage("DNS 压缩指针无效")
                }
                endOffset = endOffset ?? position + 2
                position = target
                pointerCount += 1
                guard pointerCount <= 128 else {
                    throw DNSProxyError.invalidMessage("DNS 压缩指针过深")
                }
                continue
            }
            guard length & 0xC0 == 0, length <= 63 else {
                throw DNSProxyError.invalidMessage("DNS 标签长度无效")
            }
            position += 1
            if length == 0 {
                endOffset = endOffset ?? position
                break
            }
            guard position + length <= bytes.count else {
                throw DNSProxyError.invalidMessage("DNS 标签越界")
            }
            expandedLength += length + 1
            guard expandedLength <= 255 else {
                throw DNSProxyError.invalidMessage("DNS 名称过长")
            }
            labels.append(escapeLabel(bytes[position ..< position + length]))
            position += length
        }
        guard let endOffset else {
            throw DNSProxyError.invalidMessage("DNS 名称缺少结束标记")
        }
        return (labels.isEmpty ? "." : labels.joined(separator: "."), endOffset)
    }

    private static func escapeLabel(
        _ bytes: ArraySlice<UInt8>
    ) -> String {
        bytes.map { byte in
            switch byte {
            case 65 ... 90:
                String(UnicodeScalar(byte + 32))
            case 97 ... 122, 48 ... 57, 45, 95:
                String(UnicodeScalar(byte))
            default:
                String(format: "\\%03d", byte)
            }
        }.joined()
    }

    private static func readUInt16(
        _ bytes: [UInt8],
        at offset: Int
    ) -> UInt16 {
        UInt16(bytes[offset]) << 8 | UInt16(bytes[offset + 1])
    }
}
