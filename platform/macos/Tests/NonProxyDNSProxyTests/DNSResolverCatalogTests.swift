@testable import NonProxyDNSProxy
import NetworkExtension
import XCTest

final class DNSResolverCatalogTests: XCTestCase {
    func testSelectsMostSpecificSplitDNSResolver() {
        let defaults = NEDNSSettings(servers: ["1.1.1.1"])
        let corporate = NEDNSSettings(servers: ["10.0.0.53"])
        corporate.matchDomains = ["corp.example"]
        let catalog = DNSSystemResolverCatalog(
            settings: [defaults, corporate]
        )

        XCTAssertEqual(
            catalog.upstreams(for: "service.corp.example"),
            [DNSUpstreamEndpoint(ipAddress: "10.0.0.53")]
        )
        XCTAssertEqual(
            catalog.upstreams(for: "www.example.com"),
            [DNSUpstreamEndpoint(ipAddress: "1.1.1.1")]
        )
    }

    func testParsesScopedIPv6AndExplicitPort() {
        XCTAssertEqual(
            DNSUpstreamEndpoint.parse("[2001:db8::53]:5353"),
            DNSUpstreamEndpoint(
                ipAddress: "2001:db8::53",
                port: 5353
            )
        )
        XCTAssertNil(DNSUpstreamEndpoint.parse("not-an-address"))
        XCTAssertNil(DNSUpstreamEndpoint.parse("1.1.1.1:0"))
    }
}
