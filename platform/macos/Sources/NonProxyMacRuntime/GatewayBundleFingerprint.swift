import Foundation

public enum GatewayBundleFingerprint {
    public static let environmentKey =
        "NONPROXY_GATEWAY_BUNDLE_FINGERPRINT"

    public static func live(
        bundle: Bundle = .main
    ) throws -> String {
        let plistURL = bundle.bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("LaunchAgents", isDirectory: true)
            .appendingPathComponent(
                MacSharedRuntimePaths.gatewayAgentPlistName
            )
        return try read(plistURL: plistURL)
    }

    public static func read(plistURL: URL) throws -> String {
        do {
            let data = try Data(
                contentsOf: plistURL,
                options: [.mappedIfSafe]
            )
            guard let root = try PropertyListSerialization
                .propertyList(from: data, options: [], format: nil)
                    as? [String: Any],
                  let environment =
                    root["EnvironmentVariables"] as? [String: Any],
                  let fingerprint =
                    environment[environmentKey] as? String,
                  isCanonical(fingerprint)
            else {
                throw GatewayBundleFingerprintError.invalidFingerprint
            }
            return fingerprint
        } catch let error as GatewayBundleFingerprintError {
            throw error
        } catch {
            throw GatewayBundleFingerprintError.unreadablePlist
        }
    }

    private static func isCanonical(_ value: String) -> Bool {
        value.utf8.count == 64
            && value.utf8.allSatisfy {
                ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)
            }
    }
}

public enum GatewayBundleFingerprintError:
    Error, Equatable, Sendable
{
    case unreadablePlist
    case invalidFingerprint
}
