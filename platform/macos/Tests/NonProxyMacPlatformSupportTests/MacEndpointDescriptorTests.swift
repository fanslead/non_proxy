import Network
@testable import NonProxyMacPlatformSupport
import NonProxyProviderContracts
import XCTest

final class MacEndpointDescriptorTests: XCTestCase {
    func testKeepsConnectByNameDomainAlongsideResolvedAddress() throws {
        let endpoint = NWEndpoint.hostPort(
            host: .ipv4(IPv4Address("203.0.113.10")!),
            port: 443
        )

        let descriptor = try MacEndpointDescriptor(
            endpoint: endpoint,
            connectByName: "API.Example.COM."
        )

        XCTAssertEqual(descriptor.normalizedDomain, "api.example.com")
        XCTAssertEqual(descriptor.ipAddress, "203.0.113.10")
        XCTAssertEqual(descriptor.port, 443)
        XCTAssertEqual(
            descriptor.policyDestination(transport: .tcp).transport,
            .tcp
        )
    }

    func testUsesEndpointHostnameWhenOriginalNameIsUnavailable() throws {
        let endpoint = NWEndpoint.hostPort(
            host: .name("例子.测试", nil),
            port: 8443
        )

        let descriptor = try MacEndpointDescriptor(
            endpoint: endpoint,
            connectByName: nil
        )

        XCTAssertEqual(
            descriptor.normalizedDomain,
            "xn--fsqu00a.xn--0zwm56d"
        )
        XCTAssertNil(descriptor.ipAddress)
    }
}
