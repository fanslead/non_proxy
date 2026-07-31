import Foundation
import NonProxyProviderContracts
import NonProxyProviderCore

public struct MacProviderRuntimeComponents: Sendable {
    public let runtime: ProviderPolicyRuntime
    public let lifecycle: ProviderLifecycleCoordinator
    public let control: ProviderControlClient
    public let decisions: ProviderDecisionReporter

    public init(
        runtime: ProviderPolicyRuntime,
        lifecycle: ProviderLifecycleCoordinator,
        control: ProviderControlClient,
        decisions: ProviderDecisionReporter
    ) {
        self.runtime = runtime
        self.lifecycle = lifecycle
        self.control = control
        self.decisions = decisions
    }
}

public enum MacProviderBootstrap {
    public static func make(
        kind: Nonproxy_Provider_V1_ProviderKind,
        paths: MacProviderPaths,
        bundle: Bundle = .main,
        metricsReader: @escaping ProviderLifecycleCoordinator.MetricsReader = {
            .idle
        }
    ) throws -> MacProviderRuntimeComponents {
        let identity = try providerIdentity(kind: kind)
        let version = try bundleValue(
            "CFBundleShortVersionString",
            bundle: bundle
        )
        let buildID = try bundleValue("CFBundleVersion", bundle: bundle)
        let session = ProviderSession()
        let cache = try PolicySnapshotCache(
            directory: paths.cacheDirectory,
            providerName: identity.providerName
        )
        let configuration = try ProviderConfiguration(
            kind: kind,
            component: identity.component,
            socketPath: paths.socketPath,
            bootstrapCapability: paths.readBootstrapCapability(),
            cacheDirectory: paths.cacheDirectory,
            semanticVersion: version,
            buildID: buildID
        )
        let control = try ProviderControlClient(
            configuration: configuration,
            session: session,
            cache: cache
        )
        let runtime = ProviderPolicyRuntime()
        let decisions = ProviderDecisionReporter(control: control)
        let lifecycle = ProviderLifecycleCoordinator(
            control: control,
            session: session,
            cache: cache,
            runtime: runtime,
            metricsReader: metricsReader
        )
        return MacProviderRuntimeComponents(
            runtime: runtime,
            lifecycle: lifecycle,
            control: control,
            decisions: decisions
        )
    }

    private static func providerIdentity(
        kind: Nonproxy_Provider_V1_ProviderKind
    ) throws -> (
        component: Nonproxy_Common_V1_ComponentKind,
        providerName: String
    ) {
        switch kind {
        case .transparentProxy:
            return (.transparentProxy, "transparent-proxy")
        case .dnsProxy:
            return (.dnsProxy, "dns-proxy")
        default:
            throw ProviderError.invalidConfiguration("macOS Provider 类型无效")
        }
    }

    private static func bundleValue(
        _ key: String,
        bundle: Bundle
    ) throws -> String {
        guard let value = bundle.object(forInfoDictionaryKey: key) as? String,
              !value.isEmpty
        else {
            throw ProviderError.invalidConfiguration("Provider Bundle 版本信息不完整")
        }
        return value
    }
}
