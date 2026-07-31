import Foundation
import Testing

@testable import NonProxyMacHostBridge

@MainActor
struct SystemProxyDiscoveryControllerTests {
    @Test
    func readsOnlyEnabledSupportedEndpointsAndDeduplicatesHTTP() throws {
        let proxies = SystemProxyDiscoveryController.discover(values: [
            "SOCKSEnable": 1,
            "SOCKSProxy": " 127.0.0.1 ",
            "SOCKSPort": 7891,
            "HTTPEnable": true,
            "HTTPProxy": "proxy.example",
            "HTTPPort": 8080,
            "HTTPSEnable": 1,
            "HTTPSProxy": "PROXY.EXAMPLE",
            "HTTPSPort": 8080,
            "ProxyUser": "alice",
            "ProxyPassword": "private",
        ])

        #expect(proxies.count == 2)
        #expect(proxies[0].kind == "socks5")
        #expect(proxies[0].host == "127.0.0.1")
        #expect(proxies[0].port == 7891)
        #expect(proxies[1].kind == "http_connect")

        let payload = SystemProxyDiscoveryPayload.result(proxies: proxies)
        let json = String(
            decoding: try JSONEncoder().encode(payload),
            as: UTF8.self
        )
        #expect(!json.contains("alice"))
        #expect(!json.contains("private"))
    }

    @Test
    func rejectsDisabledMalformedAndOutOfRangeEntries() {
        let proxies = SystemProxyDiscoveryController.discover(values: [
            "SOCKSEnable": 0,
            "SOCKSProxy": "127.0.0.1",
            "SOCKSPort": 1080,
            "HTTPEnable": 1,
            "HTTPProxy": "\u{0}unsafe",
            "HTTPPort": 8080,
            "HTTPSEnable": 1,
            "HTTPSProxy": "proxy.example",
            "HTTPSPort": 70000,
        ])

        #expect(proxies.isEmpty)
    }
}
