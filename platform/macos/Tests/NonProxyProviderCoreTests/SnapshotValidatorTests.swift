import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import SwiftProtobuf
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
        payload.formatVersion = SnapshotValidator.networkProfilePayloadVersion
        payload.capabilities = Nonproxy_Policy_V1_CompileCapabilitySet()

        let hash = try CanonicalSnapshotHasher.hash(
            schemaVersion: 1,
            payload: payload
        )

        XCTAssertEqual(
            hash,
            Data(hex: "e69cae04e22406132514bc8386d7e18e4204369b9eaf3582ea0f2565ec3e2f78")
        )
    }

    func testRuntimeOverrideHasherMatchesRustGoldenVector() throws {
        var runtimeOverride = Nonproxy_Policy_V1_RuntimeRoutingOverride()
        runtimeOverride.mode = .paused
        var expiresAt = Google_Protobuf_Timestamp()
        expiresAt.seconds = 2
        runtimeOverride.expiresAt = expiresAt
        var payload = SnapshotFixtures.payload(
            capabilities: Nonproxy_Policy_V1_CompileCapabilitySet()
        )
        payload.runtimeOverride = runtimeOverride

        let hash = try CanonicalSnapshotHasher.hash(
            schemaVersion: 1,
            payload: payload
        )

        XCTAssertEqual(
            hash,
            Data(hex: "6de2c8f8ded0f99c1b229caf35613fb10381eb8e680f8da4dcd0d70af1326ab2")
        )
    }

    func testAcceptsLegacyPayloadWithoutNetworkCatalog() throws {
        var payload = SnapshotFixtures.payload()
        payload.formatVersion = SnapshotValidator.legacyPayloadVersion
        let snapshot = try SnapshotFixtures.snapshot(payload: payload)

        let verified = try SnapshotValidator.validate(snapshot)

        XCTAssertEqual(
            verified.payload.formatVersion,
            SnapshotValidator.legacyPayloadVersion
        )
    }

    func testAcceptsVersionTwoPayloadWithoutRuntimeOverride() throws {
        var payload = SnapshotFixtures.payload()
        payload.formatVersion = SnapshotValidator.networkProfilePayloadVersion

        let verified = try SnapshotValidator.validate(
            SnapshotFixtures.snapshot(payload: payload)
        )

        XCTAssertEqual(
            verified.payload.formatVersion,
            SnapshotValidator.networkProfilePayloadVersion
        )
        XCTAssertFalse(verified.payload.hasRuntimeOverride)
    }

    func testNetworkCatalogIsValidatedAndCoveredByHash() throws {
        let profile = networkProfile(id: "office", fingerprint: String(repeating: "a", count: 64))
        let policy = networkPolicy(profileID: profile.id)
        var payload = SnapshotFixtures.payload(policies: [policy])
        payload.networkProfiles = [profile]
        var snapshot = try SnapshotFixtures.snapshot(payload: payload)

        var tampered = payload
        tampered.networkProfiles[0].fingerprintValue = String(repeating: "b", count: 64)
        snapshot.payload = try tampered.serializedData()

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsNetworkRuleWithoutCatalogEntry() throws {
        let payload = SnapshotFixtures.payload(
            policies: [networkPolicy(profileID: "missing")]
        )
        let snapshot = try SnapshotFixtures.snapshot(payload: payload)

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsDuplicateNetworkFingerprint() throws {
        let fingerprint = String(repeating: "a", count: 64)
        var payload = SnapshotFixtures.payload()
        payload.networkProfiles = [
            networkProfile(id: "home", fingerprint: fingerprint),
            networkProfile(id: "office", fingerprint: fingerprint),
        ]
        let snapshot = try SnapshotFixtures.snapshot(payload: payload)

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
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

    func testRuntimeOverrideIsValidatedAndCoveredByHash() throws {
        var runtimeOverride = Nonproxy_Policy_V1_RuntimeRoutingOverride()
        runtimeOverride.mode = .paused
        var expiresAt = Google_Protobuf_Timestamp()
        expiresAt.seconds = 2
        runtimeOverride.expiresAt = expiresAt
        var payload = SnapshotFixtures.payload()
        payload.runtimeOverride = runtimeOverride
        var snapshot = try SnapshotFixtures.snapshot(payload: payload)

        var tampered = payload
        tampered.runtimeOverride.mode = .direct
        snapshot.payload = try tampered.serializedData()

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsRuntimeOverrideWithoutSnapshotCreationTime() throws {
        var runtimeOverride = Nonproxy_Policy_V1_RuntimeRoutingOverride()
        runtimeOverride.mode = .paused
        var expiresAt = Google_Protobuf_Timestamp()
        expiresAt.seconds = 2
        runtimeOverride.expiresAt = expiresAt
        var payload = SnapshotFixtures.payload()
        payload.runtimeOverride = runtimeOverride
        var snapshot = try SnapshotFixtures.snapshot(payload: payload)
        snapshot.metadata.clearCreatedAt()

        XCTAssertThrowsError(try SnapshotValidator.validate(snapshot))
    }

    func testRejectsProxyOverrideWithoutKnownCapableOutbound() throws {
        var runtimeOverride = Nonproxy_Policy_V1_RuntimeRoutingOverride()
        runtimeOverride.mode = .proxy
        runtimeOverride.outboundID = "missing"
        var expiresAt = Google_Protobuf_Timestamp()
        expiresAt.seconds = 2
        runtimeOverride.expiresAt = expiresAt
        var payload = SnapshotFixtures.payload()
        payload.runtimeOverride = runtimeOverride

        XCTAssertThrowsError(
            try SnapshotValidator.validate(
                SnapshotFixtures.snapshot(payload: payload)
            )
        )
    }
}

private func networkProfile(
    id: String,
    fingerprint: String
) -> Nonproxy_Policy_V1_NetworkProfileBinding {
    var profile = Nonproxy_Policy_V1_NetworkProfileBinding()
    profile.id = id
    profile.fingerprintKind = .wifiSsidSha256
    profile.fingerprintValue = fingerprint
    return profile
}

private func networkPolicy(
    profileID: String
) -> Nonproxy_Policy_V1_Policy {
    var network = Nonproxy_Policy_V1_NetworkMatcher()
    network.profileID = profileID
    var matcher = Nonproxy_Policy_V1_PolicyMatch()
    matcher.network = network
    var policy = Nonproxy_Policy_V1_Policy()
    policy.id = "network-policy"
    policy.displayName = "网络规则"
    policy.sourceKind = .network
    policy.match = matcher
    policy.decision = SnapshotFixtures.directDecision()
    policy.enabled = true
    policy.origin = .user
    policy.revision = 1
    return policy
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
