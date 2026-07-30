import Foundation
import Network
import NonProxyMacPlatformSupport
import NonProxyProviderCore

enum ProxyFlowEndpointCodec {
    static func encode(
        destination: PolicyDestination
    ) throws -> NPF1Endpoint {
        let host = destination.normalizedDomain ?? destination.ipAddress
        guard let host else {
            throw NPF1ProtocolError.invalidPayload
        }
        return try NPF1Endpoint(host: host, port: destination.port)
    }

    static func encode(endpoint: NWEndpoint) throws -> NPF1Endpoint {
        let descriptor = try MacEndpointDescriptor(
            endpoint: endpoint,
            connectByName: nil
        )
        let host = descriptor.normalizedDomain ?? descriptor.ipAddress
        guard let host else {
            throw NPF1ProtocolError.invalidPayload
        }
        return try NPF1Endpoint(host: host, port: descriptor.port)
    }

    static func decode(_ endpoint: NPF1Endpoint) throws -> NWEndpoint {
        guard let port = NWEndpoint.Port(rawValue: endpoint.port) else {
            throw NPF1ProtocolError.invalidPayload
        }
        switch endpoint {
        case .domain(let domain, _):
            return .hostPort(host: .name(domain, nil), port: port)
        case .ipv4(let bytes, _):
            guard let address = IPv4Address(bytes) else {
                throw NPF1ProtocolError.invalidPayload
            }
            return .hostPort(host: .ipv4(address), port: port)
        case .ipv6(let bytes, _):
            guard let address = IPv6Address(bytes) else {
                throw NPF1ProtocolError.invalidPayload
            }
            return .hostPort(host: .ipv6(address), port: port)
        }
    }
}
