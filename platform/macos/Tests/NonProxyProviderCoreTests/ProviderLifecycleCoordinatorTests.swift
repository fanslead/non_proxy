import Foundation
@testable import NonProxyProviderCore
import XCTest

final class ProviderLifecycleCoordinatorTests: XCTestCase {
    func testStartsWithLiveSnapshot() async throws {
        let fixture = try makeFixture(
            control: FixedControl(
                result: .success(
                    ProviderSynchronizationResult(
                        snapshot: verifiedSnapshot(),
                        currentSnapshotVersion: 1,
                        publicationState: .active
                    )
                )
            )
        )
        defer { fixture.cleanup() }

        try await fixture.coordinator.start()
        defer { fixture.coordinator.stop() }

        XCTAssertEqual(fixture.runtime.activeSnapshotVersion, 1)
        XCTAssertEqual(fixture.coordinator.status.connectivity, .connected)
        XCTAssertNil(fixture.coordinator.status.lastErrorCode)
    }

    func testUsesVerifiedCacheWhenGatewayIsUnavailable() async throws {
        let fixture = try makeFixture(
            control: FixedControl(
                result: .failure(
                    ProviderError.lifecycle("测试控制面不可用")
                )
            )
        )
        defer { fixture.cleanup() }
        try await fixture.cache.save(verifiedSnapshot())

        try await fixture.coordinator.start()
        defer { fixture.coordinator.stop() }

        XCTAssertEqual(fixture.runtime.activeSnapshotVersion, 1)
        XCTAssertEqual(
            fixture.coordinator.status.connectivity,
            .usingCachedSnapshot
        )
        XCTAssertEqual(
            fixture.coordinator.status.lastErrorCode,
            "NP_PROVIDER_LIFECYCLE_FAILED"
        )
    }

    func testFailsWithoutLiveOrCachedSnapshot() async throws {
        let fixture = try makeFixture(
            control: FixedControl(
                result: .failure(
                    ProviderError.lifecycle("测试控制面不可用")
                )
            )
        )
        defer { fixture.cleanup() }

        do {
            try await fixture.coordinator.start()
            XCTFail("无实时或缓存快照时不应启动")
        } catch let error as ProviderError {
            XCTAssertEqual(error.code, "NP_PROVIDER_LIFECYCLE_FAILED")
        }
        XCTAssertEqual(fixture.runtime.activeSnapshotVersion, 0)
    }

    func testStopDuringBootstrapCannotRestartBackgroundLoop() async throws {
        let fixture = try makeFixture(
            control: DelayedControl(
                delay: .milliseconds(100),
                result: ProviderSynchronizationResult(
                    snapshot: verifiedSnapshot(),
                    currentSnapshotVersion: 1,
                    publicationState: .active
                )
            )
        )
        defer { fixture.cleanup() }

        let startup = Task {
            try await fixture.coordinator.start()
        }
        try await Task.sleep(for: .milliseconds(10))
        fixture.coordinator.stop()

        do {
            try await startup.value
            XCTFail("停止后的启动任务不应重新创建刷新循环")
        } catch is CancellationError {
            XCTAssertEqual(fixture.coordinator.status.connectivity, .stopped)
        }
    }

    private func makeFixture<Control: ProviderControlServing>(
        control: Control
    ) throws -> LifecycleFixture {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(
                "nonproxy-lifecycle-tests-\(UUID().uuidString)",
                isDirectory: true
            )
        let cache = try PolicySnapshotCache(
            directory: directory,
            providerName: "transparent-proxy"
        )
        let runtime = ProviderPolicyRuntime()
        let coordinator = ProviderLifecycleCoordinator(
            control: control,
            session: ProviderSession(instanceID: "test-instance"),
            cache: cache,
            runtime: runtime,
            refreshInterval: .seconds(3_600)
        )
        return LifecycleFixture(
            directory: directory,
            cache: cache,
            runtime: runtime,
            coordinator: coordinator
        )
    }

    private func verifiedSnapshot() throws -> VerifiedPolicySnapshot {
        try SnapshotValidator.validate(SnapshotFixtures.snapshot(state: .active))
    }
}

private struct FixedControl: ProviderControlServing {
    let result: Result<ProviderSynchronizationResult, ProviderError>

    func synchronize(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics
    ) async throws -> ProviderSynchronizationResult {
        try result.get()
    }

    func refresh(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics
    ) async throws -> ProviderSynchronizationResult {
        try result.get()
    }
}

private struct DelayedControl: ProviderControlServing {
    let delay: Duration
    let result: ProviderSynchronizationResult

    func synchronize(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics
    ) async throws -> ProviderSynchronizationResult {
        try await Task.sleep(for: delay)
        return result
    }

    func refresh(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics
    ) async throws -> ProviderSynchronizationResult {
        result
    }
}

private struct LifecycleFixture {
    let directory: URL
    let cache: PolicySnapshotCache
    let runtime: ProviderPolicyRuntime
    let coordinator: ProviderLifecycleCoordinator

    func cleanup() {
        try? FileManager.default.removeItem(at: directory)
    }
}
