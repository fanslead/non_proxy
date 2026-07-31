import Foundation
import Testing

@testable import NonProxyMacRuntime

struct AdapterHostBundleFingerprintTests {
  @Test
  func readsOnlyTheAdapterHostFingerprintKey() throws {
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString)
      .appendingPathExtension("plist")
    defer {
      try? FileManager.default.removeItem(at: url)
    }
    let fingerprint = String(repeating: "b", count: 64)
    let root: [String: Any] = [
      "EnvironmentVariables": [
        GatewayBundleFingerprint.environmentKey:
          String(repeating: "a", count: 64),
        AdapterHostBundleFingerprint.environmentKey: fingerprint,
      ]
    ]
    let data = try PropertyListSerialization.data(
      fromPropertyList: root,
      format: .xml,
      options: 0
    )
    try data.write(to: url, options: .atomic)

    #expect(
      try AdapterHostBundleFingerprint.read(plistURL: url)
        == fingerprint
    )
  }
}
