import Foundation
import Network

public enum NPF1Endpoint: Sendable, Equatable {
    case domain(String, UInt16)
    case ipv4(Data, UInt16)
    case ipv6(Data, UInt16)

    public init(host: String, port: UInt16) throws {
        guard port > 0 else {
            throw NPF1ProtocolError.invalidPayload
        }
        if let address = IPv4Address(host) {
            self = .ipv4(address.rawValue, port)
            return
        }
        if let address = IPv6Address(host) {
            self = .ipv6(address.rawValue, port)
            return
        }
        guard let domain = DomainNameNormalizer.normalize(host),
              let bytes = domain.data(using: .utf8),
              !bytes.isEmpty,
              bytes.count <= UInt8.max
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        self = .domain(domain, port)
    }

    public var host: String {
        switch self {
        case .domain(let value, _):
            value
        case .ipv4(let bytes, _):
            IPv4Address(bytes)?.debugDescription ?? ""
        case .ipv6(let bytes, _):
            IPv6Address(bytes)?.debugDescription ?? ""
        }
    }

    public var port: UInt16 {
        switch self {
        case .domain(_, let port), .ipv4(_, let port), .ipv6(_, let port):
            port
        }
    }

    func encode(into output: inout Data) throws {
        switch self {
        case .domain(let domain, let port):
            let bytes = Data(domain.utf8)
            guard port > 0,
                  DomainNameNormalizer.normalize(domain) == domain,
                  !bytes.isEmpty,
                  let length = UInt8(exactly: bytes.count)
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            output.append(3)
            output.append(length)
            output.append(bytes)
            output.appendBigEndian(port)
        case .ipv4(let bytes, let port):
            guard port > 0,
                  bytes.count == 4,
                  IPv4Address(bytes) != nil
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            output.append(1)
            output.append(bytes)
            output.appendBigEndian(port)
        case .ipv6(let bytes, let port):
            guard port > 0,
                  bytes.count == 16,
                  IPv6Address(bytes) != nil
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            output.append(4)
            output.append(bytes)
            output.appendBigEndian(port)
        }
    }

    static func decode(_ input: Data) throws -> (Self, Int) {
        guard let kind = input.first else {
            throw NPF1ProtocolError.invalidPayload
        }
        switch kind {
        case 1:
            guard input.count >= 7 else {
                throw NPF1ProtocolError.invalidPayload
            }
            let port = try input.readUInt16(at: 5)
            guard port > 0 else {
                throw NPF1ProtocolError.invalidPayload
            }
            return (
                .ipv4(
                    input.subdata(in: 1..<5),
                    port
                ),
                7
            )
        case 3:
            guard input.count >= 2 else {
                throw NPF1ProtocolError.invalidPayload
            }
            let length = Int(input[1])
            let consumed = 4 + length
            guard length > 0, input.count >= consumed,
                  let domain = String(
                      data: input.subdata(in: 2..<(2 + length)),
                      encoding: .utf8
                  )
            else {
                throw NPF1ProtocolError.invalidPayload
            }
            let endpoint = try Self(
                host: domain,
                port: try input.readUInt16(at: consumed - 2)
            )
            guard case .domain = endpoint else {
                throw NPF1ProtocolError.invalidPayload
            }
            return (endpoint, consumed)
        case 4:
            guard input.count >= 19 else {
                throw NPF1ProtocolError.invalidPayload
            }
            let port = try input.readUInt16(at: 17)
            guard port > 0 else {
                throw NPF1ProtocolError.invalidPayload
            }
            return (
                .ipv6(
                    input.subdata(in: 1..<17),
                    port
                ),
                19
            )
        default:
            throw NPF1ProtocolError.invalidPayload
        }
    }
}

public enum NPF1PayloadCodec {
    public static let capabilityBytes = 32
    public static let minimumWindowBytes: UInt32 = 16 * 1024
    public static let maximumWindowBytes: UInt32 = 16 * 1024 * 1024
    public static let maximumDatagramBytes = 65_000

    public static func encodeOpen(
        capability: Data,
        outboundID: String,
        endpoint: NPF1Endpoint,
        initialWindowBytes: UInt32
    ) throws -> Data {
        try encodeOpen(
            capability: capability,
            proxyTarget: .outbound(id: outboundID),
            endpoint: endpoint,
            initialWindowBytes: initialWindowBytes
        )
    }

    public static func encodeOpen(
        capability: Data,
        proxyTarget: ProviderProxyTarget,
        endpoint: NPF1Endpoint,
        initialWindowBytes: UInt32
    ) throws -> Data {
        guard proxyTarget.isValid else {
            throw NPF1ProtocolError.invalidPayload
        }
        let targetID: String
        let snapshotVersion: UInt64?
        switch proxyTarget {
        case .outbound(let id):
            targetID = id
            snapshotVersion = nil
        case .group(let id, let version, _):
            targetID = id
            snapshotVersion = version
        }
        let target = Data(targetID.utf8)
        guard capability.count == capabilityBytes,
              isValidIdentifier(targetID),
              let targetLength = UInt8(exactly: target.count),
              (minimumWindowBytes...maximumWindowBytes)
                  .contains(initialWindowBytes)
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        var output = Data(capacity: 74 + target.count)
        output.append(capability)
        if let snapshotVersion {
            output.append(contentsOf: [0, 2, targetLength])
            output.append(target)
            output.appendBigEndian(snapshotVersion)
        } else {
            output.append(targetLength)
            output.append(target)
        }
        try endpoint.encode(into: &output)
        output.appendBigEndian(initialWindowBytes)
        return output
    }

    public static func encodeDatagram(
        endpoint: NPF1Endpoint,
        content: Data
    ) throws -> Data {
        guard !content.isEmpty, content.count <= maximumDatagramBytes else {
            throw NPF1ProtocolError.invalidPayload
        }
        var output = Data(capacity: content.count + 20)
        try endpoint.encode(into: &output)
        output.append(content)
        return output
    }

    public static func decodeDatagram(
        _ input: Data
    ) throws -> (endpoint: NPF1Endpoint, content: Data) {
        let (endpoint, consumed) = try NPF1Endpoint.decode(input)
        let content = input.suffix(from: consumed)
        guard !content.isEmpty, content.count <= maximumDatagramBytes else {
            throw NPF1ProtocolError.invalidPayload
        }
        return (endpoint, Data(content))
    }

    public static func encodeWindowUpdate(_ bytes: UInt32) throws -> Data {
        guard bytes > 0 else {
            throw NPF1ProtocolError.invalidPayload
        }
        var output = Data(capacity: 4)
        output.appendBigEndian(bytes)
        return output
    }

    public static func decodeWindowUpdate(_ input: Data) throws -> UInt32 {
        guard input.count == 4 else {
            throw NPF1ProtocolError.invalidPayload
        }
        let bytes = try input.readUInt32(at: 0)
        guard bytes > 0 else {
            throw NPF1ProtocolError.invalidPayload
        }
        return bytes
    }

    public static func encodeReady(
        selectedOutboundID: String,
        initialWindowBytes: UInt32
    ) throws -> Data {
        let outbound = Data(selectedOutboundID.utf8)
        guard isValidIdentifier(selectedOutboundID),
              let length = UInt8(exactly: outbound.count),
              (minimumWindowBytes...maximumWindowBytes)
                  .contains(initialWindowBytes)
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        var output = Data(capacity: 5 + outbound.count)
        output.append(length)
        output.append(outbound)
        output.appendBigEndian(initialWindowBytes)
        return output
    }

    public static func decodeReady(
        _ input: Data
    ) throws -> (selectedOutboundID: String, initialWindowBytes: UInt32) {
        guard let lengthByte = input.first else {
            throw NPF1ProtocolError.invalidPayload
        }
        let length = Int(lengthByte)
        let windowOffset = 1 + length
        guard length > 0,
              input.count == windowOffset + 4,
              let outboundID = String(
                  data: input.subdata(in: 1..<windowOffset),
                  encoding: .utf8
              ),
              isValidIdentifier(outboundID)
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        let window = try input.readUInt32(at: windowOffset)
        guard (minimumWindowBytes...maximumWindowBytes).contains(window)
        else {
            throw NPF1ProtocolError.invalidPayload
        }
        return (outboundID, window)
    }

    public static func decodeErrorCode(_ input: Data) -> String {
        guard input.count <= 128,
              let value = String(data: input, encoding: .utf8),
              value.hasPrefix("NP_"),
              value.utf8.allSatisfy({
                  (48...57).contains($0)
                      || (65...90).contains($0)
                      || $0 == 95
              })
        else {
            return "NP_PROXY_GATEWAY_ERROR"
        }
        return value
    }

    private static func isValidIdentifier(_ value: String) -> Bool {
        let bytes = value.utf8
        guard !bytes.isEmpty, bytes.count <= 128 else {
            return false
        }
        return bytes.allSatisfy { byte in
            switch byte {
            case 48...57, 65...90, 97...122, 45, 46, 58, 95:
                true
            default:
                false
            }
        }
    }
}
