import Foundation

public enum GatewayBundleFingerprint {
    public static let environmentKey =
        "NONPROXY_GATEWAY_BUNDLE_FINGERPRINT"

    public static func live(
        bundle: Bundle = .main
    ) throws -> String {
        do {
            return try LaunchAgentBundleFingerprint.live(
                plistName: MacSharedRuntimePaths.gatewayAgentPlistName,
                environmentKey: environmentKey,
                bundle: bundle
            )
        } catch {
            throw map(error)
        }
    }

    public static func read(plistURL: URL) throws -> String {
        do {
            return try LaunchAgentBundleFingerprint.read(
                plistURL: plistURL,
                environmentKey: environmentKey
            )
        } catch {
            throw map(error)
        }
    }

    private static func map(_ error: Error) -> GatewayBundleFingerprintError {
        switch error {
        case BundleFingerprintError.invalidFingerprint:
            .invalidFingerprint
        default:
            .unreadablePlist
        }
    }
}

public enum GatewayBundleFingerprintError:
    Error, Equatable, Sendable
{
    case unreadablePlist
    case invalidFingerprint
}
