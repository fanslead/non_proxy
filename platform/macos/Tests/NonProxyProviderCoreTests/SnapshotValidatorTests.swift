import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import XCTest

final class SnapshotValidatorTests: XCTestCase {
    func testAcceptsValidEmptySnapshot() throws {
        let snapshot = try SnapshotFixtures.snapshot()

        let verified = try SnapshotValidator.validate(snapshot)

        XCTAssertEqual(verified.version, 1)
        XCTAssertTrue(verified.payload.policies.isEmpty)
    }

    func testCanonicalHasherMatchesRustGoldenVector() throws {
        var payload = SnapshotFixtures.payload()
        payload.capabilities = Nonproxy_Policy_V1_CompileCapabilitySet()

        let hash = try CanonicalSnapshotHasher.hash(
            schemaVersion: 1,
            payload: payload
        )

        XCTAssertEqual(
            hash,
            Data(hex: "5137c6ba8034894392d42d812412e5275de3e3dfe0e1e1ce64b3fa68a402f703")
        )
    }

    func testRejectsTamperedPayload() throws {
        var snapshot = try SnapshotFixtures.snapshot()
        snapshot.payload.append(0xff)

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsDuplicateOutboundCapabilities() throws {
        var outbound = Nonproxy_Policy_V1_OutboundCapabilitySpec()
        outbound.outboundID = "proxy"
        outbound.transports = [.tcp]
        outbound.ipFamilies = [.ipv4]
        var capabilities = SnapshotFixtures.fullCapabilities()
        capabilities.outbounds = [outbound, outbound]
        let payload = SnapshotFixtures.payload(capabilities: capabilities)
        let snapshot = try SnapshotFixtures.snapshot(payload: payload)

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsSemanticallyInvalidDomainWithMatchingHash() throws {
        let policy = SnapshotFixtures.sitePolicy(pattern: "Example.COM")
        let payload = SnapshotFixtures.payload(policies: [policy])
        let snapshot = try SnapshotFixtures.snapshot(payload: payload)

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsUnknownProxyOutboundWithMatchingHash() throws {
        let policy = SnapshotFixtures.sitePolicy(action: .proxy)
        let payload = SnapshotFixtures.payload(policies: [policy])
        let snapshot = try SnapshotFixtures.snapshot(payload: payload)

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }
}

private extension Data {
    init?(hex: String) {
        guard hex.count.isMultiple(of: 2) else {
            return nil
        }
        var bytes = [UInt8]()
        bytes.reserveCapacity(hex.count / 2)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index ..< next], radix: 16) else {
                return nil
            }
            bytes.append(byte)
            index = next
        }
        self.init(bytes)
    }
}
