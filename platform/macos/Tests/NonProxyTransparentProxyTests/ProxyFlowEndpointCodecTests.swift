import Foundation
import Network
import NonProxyProviderContracts
import NonProxyProviderCore
@testable import NonProxyTransparentProxy
import XCTest

final class ProxyFlowEndpointCodecTests: XCTestCase {
    func testPrefersNormalizedDomainFromPolicyDestination() throws {
        let destination = PolicyDestination(
            normalizedDomain: "example.test",
            registrableDomain: nil,
            ipAddress: "192.0.2.1",
            transport: .tcp,
            port: 443
        )

        let endpoint = try ProxyFlowEndpointCodec.encode(
            destination: destination
        )

        XCTAssertEqual(endpoint, .domain("example.test", 443))
    }

    func testRoundTripsIPv4IPv6AndDomainEndpoints() throws {
        let values: [NPF1Endpoint] = [
            try NPF1Endpoint(host: "dns.example", port: 53),
            try NPF1Endpoint(host: "192.0.2.1", port: 53),
            try NPF1Endpoint(host: "2001:db8::1", port: 53),
        ]

        for value in values {
            let network = try ProxyFlowEndpointCodec.decode(value)
            let encoded = try ProxyFlowEndpointCodec.encode(
                endpoint: network
            )
            XCTAssertEqual(encoded, value)
        }
    }

    func testRejectsDestinationWithoutHost() {
        let destination = PolicyDestination(
            normalizedDomain: nil,
            registrableDomain: nil,
            ipAddress: nil,
            transport: .udp,
            port: 53
        )

        XCTAssertThrowsError(
            try ProxyFlowEndpointCodec.encode(destination: destination)
        )
    }
}
