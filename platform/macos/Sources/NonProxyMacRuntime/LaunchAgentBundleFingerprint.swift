import Foundation

enum LaunchAgentBundleFingerprint {
  static func live(
    plistName: String,
    environmentKey: String,
    bundle: Bundle
  ) throws -> String {
    let plistURL = bundle.bundleURL
      .appendingPathComponent("Contents", isDirectory: true)
      .appendingPathComponent("Library", isDirectory: true)
      .appendingPathComponent("LaunchAgents", isDirectory: true)
      .appendingPathComponent(plistName)
    return try read(
      plistURL: plistURL,
      environmentKey: environmentKey
    )
  }

  static func read(
    plistURL: URL,
    environmentKey: String
  ) throws -> String {
    do {
      let data = try Data(
        contentsOf: plistURL,
        options: [.mappedIfSafe]
      )
      guard
        let root = try PropertyListSerialization
          .propertyList(from: data, options: [], format: nil)
          as? [String: Any],
        let environment =
          root["EnvironmentVariables"] as? [String: Any],
        let fingerprint =
          environment[environmentKey] as? String,
        isCanonical(fingerprint)
      else {
        throw BundleFingerprintError.invalidFingerprint
      }
      return fingerprint
    } catch let error as BundleFingerprintError {
      throw error
    } catch {
      throw BundleFingerprintError.unreadablePlist
    }
  }

  private static func isCanonical(_ value: String) -> Bool {
    value.utf8.count == 64
      && value.utf8.allSatisfy {
        ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)
      }
  }
}

enum BundleFingerprintError: Error, Equatable, Sendable {
  case unreadablePlist
  case invalidFingerprint
}
