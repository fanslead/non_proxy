import Foundation
import Testing

@testable import NonProxyMacRuntime

struct AdapterHostRuntimeReadinessTests {
  @Test
  func inspectsTheIsolatedAdapterHostStateDirectory() throws {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    let paths = try MacSharedRuntimePaths(stateDirectory: root)
    try FileManager.default.createDirectory(
      at: paths.adapterHostStateDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.setAttributes(
      [.posixPermissions: 0o700],
      ofItemAtPath: paths.adapterHostStateDirectory.path
    )
    defer {
      try? FileManager.default.removeItem(at: root)
    }

    #expect(throws: AdapterHostRuntimeReadinessError.invalidSocket) {
      try AdapterHostRuntimeReadiness.inspect(
        paths: paths,
        expectedFingerprint: String(repeating: "b", count: 64)
      )
    }
  }
}
