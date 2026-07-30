import Darwin
import Foundation
import Network
import NonProxyProviderContracts

public struct DNSUpstreamEndpoint: Equatable, Hashable, Sendable {
    public let ipAddress: String
    public let port: UInt16
    public let scopeID: UInt32

    public init(ipAddress: String, port: UInt16 = 53, scopeID: UInt32 = 0) {
        self.ipAddress = ipAddress
        self.port = port
        self.scopeID = scopeID
    }

    public var protobuf: Nonproxy_Provider_V1_DnsUpstreamEndpoint {
        var endpoint = Nonproxy_Provider_V1_DnsUpstreamEndpoint()
        endpoint.ipAddress = ipAddress
        endpoint.port = UInt32(port)
        endpoint.scopeID = scopeID
        return endpoint
    }

    public static func parse(_ value: String) -> Self? {
        guard !value.isEmpty,
              value == value.trimmingCharacters(in: .whitespacesAndNewlines)
        else {
            return nil
        }
        let hostAndPort = splitHostAndPort(value)
        guard let hostAndPort,
              let port = UInt16(exactly: hostAndPort.port),
              port > 0
        else {
            return nil
        }
        let scoped = splitScope(hostAndPort.host)
        guard let scoped else {
            return nil
        }
        if IPv4Address(scoped.address) != nil {
            guard scoped.scopeID == 0 else {
                return nil
            }
        } else if IPv6Address(scoped.address) == nil {
            return nil
        }
        return Self(
            ipAddress: scoped.address,
            port: port,
            scopeID: scoped.scopeID
        )
    }

    private static func splitHostAndPort(
        _ value: String
    ) -> (host: String, port: Int)? {
        if value.hasPrefix("[") {
            guard let closing = value.firstIndex(of: "]") else {
                return nil
            }
            let host = String(value[value.index(after: value.startIndex) ..< closing])
            let suffix = value[value.index(after: closing)...]
            if suffix.isEmpty {
                return (host, 53)
            }
            guard suffix.first == ":",
                  let port = Int(suffix.dropFirst())
            else {
                return nil
            }
            return (host, port)
        }
        if value.filter({ $0 == ":" }).count == 1,
           let separator = value.lastIndex(of: ":"),
           let port = Int(value[value.index(after: separator)...]) {
            return (String(value[..<separator]), port)
        }
        return (value, 53)
    }

    private static func splitScope(
        _ value: String
    ) -> (address: String, scopeID: UInt32)? {
        guard let separator = value.lastIndex(of: "%") else {
            return (value, 0)
        }
        let address = String(value[..<separator])
        let scope = String(value[value.index(after: separator)...])
        guard !address.isEmpty, !scope.isEmpty else {
            return nil
        }
        if let numeric = UInt32(scope), numeric > 0 {
            return (address, numeric)
        }
        let index = if_nametoindex(scope)
        guard index > 0 else {
            return nil
        }
        return (address, index)
    }
}
