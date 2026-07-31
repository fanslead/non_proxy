import Foundation
import Testing
@testable import NonProxyMacRuntime

struct MacSharedRuntimePathsTests {
    @Test
    func derivesEveryRuntimeFileFromOneStateDirectory() throws {
        let paths = try MacSharedRuntimePaths(
            stateDirectory: URL(fileURLWithPath: "/tmp/nonproxy-runtime")
        )

        #expect(paths.controlSocket.path
            == "/tmp/nonproxy-runtime/gatewayd.sock")
        #expect(paths.flowSocket.path
            == "/tmp/nonproxy-runtime/gatewayd-flow.sock")
        #expect(paths.controlCapability.path
            == "/tmp/nonproxy-runtime/session.capability")
        #expect(paths.providerCapability.path
            == "/tmp/nonproxy-runtime/provider.capability")
        #expect(paths.runtimeIdentity.path
            == "/tmp/nonproxy-runtime/gateway.runtime.json")
        #expect(paths.adapterHostStateDirectory.path
            == "/tmp/nonproxy-runtime/adapter-host")
        #expect(paths.adapterHostSocket.path
            == "/tmp/nonproxy-runtime/adapter-host/adapter-host.sock")
        #expect(paths.adapterHostCapability.path
            == "/tmp/nonproxy-runtime/adapter-host/adapter.capability")
        #expect(paths.adapterHostRuntimeIdentity.path
            == "/tmp/nonproxy-runtime/adapter-host/adapter.runtime.json")
        #expect(paths.providerCacheDirectory.path
            == "/tmp/nonproxy-runtime/provider-cache")
    }

    @Test
    func rejectsRelativeStateDirectory() {
        guard let relativeURL = URL(string: "relative/path") else {
            Issue.record("无法构造相对测试 URL")
            return
        }
        #expect(throws: MacRuntimePathError.invalidStateDirectory) {
            try MacSharedRuntimePaths(
                stateDirectory: relativeURL
            )
        }
    }
}
