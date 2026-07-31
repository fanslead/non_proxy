import Network
@testable import NonProxyMacNetworkIdentity
import XCTest

final class MacNetworkEnvironmentMonitorTests: XCTestCase {
    func testBuildsPrivacySafeFingerprintsInSpecificityOrder() throws {
        let fingerprints = MacNetworkFingerprintFactory.make(
            interfaceClass: "wifi",
            wifiSSID: "Office WiFi",
            defaultGateway: "192.168.1.1"
        )

        XCTAssertEqual(fingerprints.map(\.kind), [
            .wifiSSIDHash,
            .defaultGatewayHash,
            .interfaceClass,
        ])
        XCTAssertEqual(
            fingerprints[0].value,
            "95e986531d4972a782f3a2a868cbecb194a0e0fc14f95280706077e9fbf63fc5"
        )
        XCTAssertEqual(
            fingerprints[1].value,
            "6bf6ef9450e91a54fce8eae50991501a8ecb4a6be61813dfceaea70f335a7948"
        )
        XCTAssertEqual(fingerprints[2].value, "wifi")
    }

    func testCanonicalizesGatewayBeforeHashing() {
        XCTAssertEqual(
            MacNetworkFingerprintFactory.defaultGateway("2001:0db8::0001"),
            MacNetworkFingerprintFactory.defaultGateway("2001:db8::1")
        )
        XCTAssertNil(MacNetworkFingerprintFactory.defaultGateway("router.local"))
    }

    func testUsesLinkLayerAddressToDisambiguateCommonGatewayIPs() {
        let first = MacNetworkFingerprintFactory.defaultGateway(
            "192.168.1.1",
            hardwareAddress: "00:09:80:01:E9:A4"
        )
        let second = MacNetworkFingerprintFactory.defaultGateway(
            "192.168.1.1",
            hardwareAddress: "00:09:80:01:E9:A5"
        )

        XCTAssertEqual(
            first?.value,
            "8906c638738fba0eeb8c436dc299a1ba2ada86a7cd732289fa8204e82275dfd7"
        )
        XCTAssertNotEqual(first, second)
    }

    func testPrioritizesOnlyPhysicalOutboundInterfaces() {
        XCTAssertEqual(
            MacNetworkEnvironmentMonitor.priority(for: .wiredEthernet),
            0
        )
        XCTAssertEqual(
            MacNetworkEnvironmentMonitor.priority(for: .wifi),
            1
        )
        XCTAssertEqual(
            MacNetworkEnvironmentMonitor.priority(for: .cellular),
            2
        )
        XCTAssertNil(MacNetworkEnvironmentMonitor.priority(for: .other))
        XCTAssertNil(MacNetworkEnvironmentMonitor.priority(for: .loopback))
    }

    func testRanksUsedTypeBeforePriorityAndGatewayBeforeInterfaceName() {
        let ranks = [
            MacNetworkInterfaceRank(
                isUsed: true,
                priority: 1,
                hasDefaultGateway: false,
                name: "awdl0"
            ),
            MacNetworkInterfaceRank(
                isUsed: false,
                priority: 0,
                hasDefaultGateway: true,
                name: "en5"
            ),
            MacNetworkInterfaceRank(
                isUsed: true,
                priority: 1,
                hasDefaultGateway: true,
                name: "en0"
            ),
        ].sorted()

        XCTAssertEqual(ranks.map(\.name), ["en0", "awdl0", "en5"])
    }

    func testDnsCachePartitionDoesNotExposeStableNetworkProfile() throws {
        let fingerprint = try XCTUnwrap(
            MacNetworkFingerprintFactory.interfaceClass("wifi")
        )
        let monitor = MacNetworkEnvironmentMonitor(
            initialInterfaceIndex: 7,
            initialFingerprints: [fingerprint]
        )
        let network = monitor.snapshot()

        let first = network.dnsCachePartitionID(
            resolverKeys: ["1.1.1.1:53:0"]
        )
        let second = network.dnsCachePartitionID(
            resolverKeys: ["8.8.8.8:53:0"]
        )

        XCTAssertEqual(network.preferredInterfaceIndex, 7)
        XCTAssertEqual(network.fingerprints.map(\.value), ["wifi"])
        XCTAssertTrue(first.hasPrefix("dns-"))
        XCTAssertNotEqual(first, second)
        XCTAssertFalse(first.contains("wifi"))
    }
}
