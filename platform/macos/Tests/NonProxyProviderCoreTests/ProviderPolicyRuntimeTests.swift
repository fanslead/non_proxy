import Foundation
import NonProxyProviderContracts
@testable import NonProxyProviderCore
import XCTest

final class ProviderPolicyRuntimeTests: XCTestCase {
    func testInstallsAndUsesLatestImmutableSnapshot() throws {
        let runtime = ProviderPolicyRuntime()
        let first = try verifiedSnapshot(version: 1, action: .block)
        let second = try verifiedSnapshot(version: 2, action: .direct)

        XCTAssertTrue(try runtime.install(first))
        XCTAssertTrue(try runtime.install(second))
        XCTAssertEqual(runtime.activeSnapshotVersion, 2)
        XCTAssertEqual(try runtime.decide(context: context()).result.action, .direct)
    }

    func testRejectsSnapshotDowngrade() throws {
        let runtime = ProviderPolicyRuntime()
        try runtime.install(verifiedSnapshot(version: 2, action: .direct))

        XCTAssertThrowsError(
            try runtime.install(verifiedSnapshot(version: 1, action: .direct))
        ) { error in
            XCTAssertEqual(
                (error as? ProviderError)?.code,
                "NP_PROVIDER_SNAPSHOT_INVALID"
            )
        }
        XCTAssertEqual(runtime.activeSnapshotVersion, 2)
    }

    func testRejectsSameVersionWithDifferentHash() throws {
        let runtime = ProviderPolicyRuntime()
        try runtime.install(verifiedSnapshot(version: 4, action: .direct))

        XCTAssertThrowsError(
            try runtime.install(verifiedSnapshot(version: 4, action: .block))
        )
    }

    private func verifiedSnapshot(
        version: UInt64,
        action: Nonproxy_Common_V1_RouteAction
    ) throws -> VerifiedPolicySnapshot {
        var decision = SnapshotFixtures.directDecision()
        decision.action = action
        let payload = SnapshotFixtures.payload(defaultDecision: decision)
        return try SnapshotValidator.validate(
            SnapshotFixtures.snapshot(payload: payload, version: version)
        )
    }

    private func context() -> PolicyConnectionContext {
        PolicyConnectionContext(
            app: .unknown,
            destination: PolicyDestination(
                normalizedDomain: "example.com",
                registrableDomain: "example.com",
                ipAddress: "203.0.113.10",
                transport: .tcp,
                port: 443
            )
        )
    }
}
