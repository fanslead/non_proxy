import Foundation
import NonProxyProviderContracts
import NonProxyProviderCore

@main
struct ProviderSmoke {
    static func main() async {
        do {
            let arguments = try Arguments.parse(CommandLine.arguments)
            let capability = try Data(contentsOf: arguments.capabilityFile)
            let configuration = try ProviderConfiguration(
                kind: arguments.kind,
                component: arguments.component,
                socketPath: arguments.socketPath,
                bootstrapCapability: capability,
                cacheDirectory: arguments.cacheDirectory,
                semanticVersion: "0.1.0",
                buildID: "provider-smoke"
            )
            let session = ProviderSession()
            let cache = try PolicySnapshotCache(
                directory: arguments.cacheDirectory,
                providerName: arguments.providerName
            )
            let client = ProviderControlClient(
                configuration: configuration,
                session: session,
                cache: cache
            )

            let result = try await client.synchronize(knownSnapshotVersion: 0)
            guard result.currentSnapshotVersion == 1,
                  result.publicationState == arguments.expectedState
            else {
                throw SmokeError.unexpectedState
            }
            print(
                "Provider 跨语言联调通过：\(arguments.providerName) 已确认快照 "
                    + "\(result.currentSnapshotVersion)，状态 "
                    + "\(arguments.expectedState)。"
            )
        } catch {
            FileHandle.standardError.write(
                Data(
                    (
                        "Provider 跨语言联调失败：\(error.localizedDescription)\n"
                            + "详细错误：\(String(reflecting: error))\n"
                    ).utf8
                )
            )
            Foundation.exit(1)
        }
    }
}

private struct Arguments {
    let socketPath: String
    let capabilityFile: URL
    let cacheDirectory: URL
    let kind: Nonproxy_Provider_V1_ProviderKind
    let component: Nonproxy_Common_V1_ComponentKind
    let providerName: String
    let expectedState: Nonproxy_Policy_V1_SnapshotState

    static func parse(_ values: [String]) throws -> Self {
        guard values.count == 6 else {
            throw SmokeError.invalidArguments
        }
        let provider = values[4]
        let expected = values[5]
        let providerValues: (
            Nonproxy_Provider_V1_ProviderKind,
            Nonproxy_Common_V1_ComponentKind
        )
        switch provider {
        case "transparent-proxy":
            providerValues = (.transparentProxy, .transparentProxy)
        case "dns-proxy":
            providerValues = (.dnsProxy, .dnsProxy)
        default:
            throw SmokeError.invalidArguments
        }
        let expectedState: Nonproxy_Policy_V1_SnapshotState
        switch expected {
        case "pending":
            expectedState = .pendingAck
        case "active":
            expectedState = .active
        default:
            throw SmokeError.invalidArguments
        }
        return Self(
            socketPath: values[1],
            capabilityFile: URL(fileURLWithPath: values[2]),
            cacheDirectory: URL(fileURLWithPath: values[3], isDirectory: true),
            kind: providerValues.0,
            component: providerValues.1,
            providerName: provider,
            expectedState: expectedState
        )
    }
}

private enum SmokeError: Error, LocalizedError {
    case invalidArguments
    case unexpectedState

    var errorDescription: String? {
        switch self {
        case .invalidArguments:
            "参数应为：socket capability cache provider expected-state"
        case .unexpectedState:
            "Provider ACK 后的快照状态不符合预期"
        }
    }
}
