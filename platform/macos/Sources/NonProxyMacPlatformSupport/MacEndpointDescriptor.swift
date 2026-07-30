import Foundation
import Network
import NonProxyProviderContracts
import NonProxyProviderCore

public struct MacEndpointDescriptor: Sendable, Equatable {
    public let normalizedDomain: String?
    public let ipAddress: String?
    public let port: UInt16

    public init(
        endpoint: NWEndpoint,
        connectByName: String?
    ) throws {
        guard case .hostPort(let host, let nwPort) = endpoint else {
            throw ProviderError.lifecycle("Provider 收到不支持的目标端点类型")
        }
        let endpointHost: String
        switch host {
        case .name(let value, _):
            endpointHost = value
        case .ipv4(let address):
            endpointHost = address.debugDescription
        case .ipv6(let address):
            endpointHost = address.debugDescription
        @unknown default:
            throw ProviderError.lifecycle("Provider 收到未知的目标主机类型")
        }
        let endpointAddress = Self.normalizedAddress(endpointHost)
        let endpointDomain = DomainNameNormalizer.normalize(endpointHost)
        let originalDomain = DomainNameNormalizer.normalize(connectByName)
        let port = nwPort.rawValue
        guard port > 0, endpointAddress != nil || endpointDomain != nil else {
            throw ProviderError.lifecycle("Provider 收到无效的目标地址或端口")
        }

        normalizedDomain = originalDomain ?? endpointDomain
        ipAddress = endpointAddress
        self.port = port
    }

    public func policyDestination(
        transport: Nonproxy_Common_V1_TransportProtocol
    ) -> PolicyDestination {
        PolicyDestination(
            normalizedDomain: normalizedDomain,
            registrableDomain: nil,
            ipAddress: ipAddress,
            transport: transport,
            port: port
        )
    }

    private static func normalizedAddress(_ value: String) -> String? {
        if let address = IPv4Address(value) {
            return address.debugDescription
        }
        if let address = IPv6Address(value) {
            return address.debugDescription
        }
        return nil
    }
}
