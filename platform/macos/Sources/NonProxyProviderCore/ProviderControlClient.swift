import Foundation
import GRPCCore
import GRPCNIOTransportHTTP2Posix
import NonProxyProviderContracts

public struct ProviderSynchronizationResult: Sendable {
    public let snapshot: VerifiedPolicySnapshot?
    public let currentSnapshotVersion: UInt64
    public let publicationState: Nonproxy_Policy_V1_SnapshotState?

    public init(
        snapshot: VerifiedPolicySnapshot?,
        currentSnapshotVersion: UInt64,
        publicationState: Nonproxy_Policy_V1_SnapshotState?
    ) {
        self.snapshot = snapshot
        self.currentSnapshotVersion = currentSnapshotVersion
        self.publicationState = publicationState
    }
}

public struct ProviderControlClient: ProviderControlServing, Sendable {
    private let configuration: ProviderConfiguration
    private let session: ProviderSession
    private let cache: PolicySnapshotCache

    public init(
        configuration: ProviderConfiguration,
        session: ProviderSession,
        cache: PolicySnapshotCache
    ) {
        self.configuration = configuration
        self.session = session
        self.cache = cache
    }

    public func synchronize(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics = .idle
    ) async throws -> ProviderSynchronizationResult {
        try await execute(
            knownSnapshotVersion: knownSnapshotVersion,
            metrics: metrics,
            mustRegister: true
        )
    }

    public func refresh(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics = .idle
    ) async throws -> ProviderSynchronizationResult {
        try await execute(
            knownSnapshotVersion: knownSnapshotVersion,
            metrics: metrics,
            mustRegister: false
        )
    }

    private func execute(
        knownSnapshotVersion: UInt64,
        metrics: ProviderHealthMetrics,
        mustRegister: Bool
    ) async throws -> ProviderSynchronizationResult {
        let transport = try HTTP2ClientTransport.Posix(
            target: .unixDomainSocket(
                path: configuration.socketPath,
                authority: "localhost"
            ),
            transportSecurity: .plaintext
        )
        return try await withGRPCClient(transport: transport) { grpcClient in
            let client = Nonproxy_Provider_V1_ProviderService.Client(
                wrapping: grpcClient
            )
            if mustRegister {
                let registration = try await register(using: client)
                try await session.install(response: registration)
            }

            var getRequest = Nonproxy_Provider_V1_GetCurrentSnapshotRequest()
            getRequest.context = try await session.requestContext()
            getRequest.knownSnapshotVersion = knownSnapshotVersion
            let response = try await client.getCurrentSnapshot(
                request: ClientRequest(message: getRequest)
            )
            if response.unchanged {
                try await reportReady(
                    version: knownSnapshotVersion,
                    metrics: metrics,
                    using: client
                )
                return ProviderSynchronizationResult(
                    snapshot: nil,
                    currentSnapshotVersion: knownSnapshotVersion,
                    publicationState: nil
                )
            }
            guard response.hasSnapshot else {
                throw ProviderError.invalidSnapshot(
                    "gatewayd 未返回快照且未声明内容未变化"
                )
            }

            let verified: VerifiedPolicySnapshot
            do {
                verified = try SnapshotValidator.validate(response.snapshot)
                try await cache.save(verified)
            } catch {
                if response.snapshot.metadata.state == .pendingAck {
                    try await reject(response.snapshot, error: error, using: client)
                }
                throw error
            }
            let publicationState: Nonproxy_Policy_V1_SnapshotState
            if response.snapshot.metadata.state == .pendingAck {
                publicationState = try await acknowledge(
                    verified,
                    using: client
                )
            } else {
                publicationState = .active
            }
            try await reportReady(
                version: verified.version,
                metrics: metrics,
                using: client
            )
            return ProviderSynchronizationResult(
                snapshot: verified,
                currentSnapshotVersion: verified.version,
                publicationState: publicationState
            )
        }
    }

    private func register<Transport: ClientTransport>(
        using client: Nonproxy_Provider_V1_ProviderService.Client<Transport>
    ) async throws -> Nonproxy_Provider_V1_RegisterProviderResponse {
        var request = Nonproxy_Provider_V1_RegisterProviderRequest()
        request.providerInstanceID = session.instanceID
        request.kind = configuration.kind
        request.version = configuration.componentVersion
        request.capabilities = ["snapshot-v1", "heartbeat-v1"]
        request.startupNonce = secureRandomBytes(count: 32)
        request.bootstrapCapability = configuration.bootstrapCapability
        return try await client.registerProvider(
            request: ClientRequest(message: request)
        )
    }

    private func acknowledge<Transport: ClientTransport>(
        _ snapshot: VerifiedPolicySnapshot,
        using client: Nonproxy_Provider_V1_ProviderService.Client<Transport>
    ) async throws -> Nonproxy_Policy_V1_SnapshotState {
        var request = Nonproxy_Provider_V1_AcknowledgeSnapshotRequest()
        request.context = try await session.requestContext()
        request.snapshotVersion = snapshot.version
        request.contentHash = snapshot.contentHash
        request.accepted = true
        let response = try await client.acknowledgeSnapshot(
            request: ClientRequest(message: request)
        )
        guard response.hasSnapshot else {
            throw ProviderError.invalidSnapshot("gatewayd 未返回快照发布状态")
        }
        return response.snapshot.state
    }

    private func reject<Transport: ClientTransport>(
        _ snapshot: Nonproxy_Policy_V1_CompiledPolicySnapshot,
        error: Error,
        using client: Nonproxy_Provider_V1_ProviderService.Client<Transport>
    ) async throws {
        var detail = Nonproxy_Common_V1_ErrorDetail()
        if let providerError = error as? ProviderError {
            detail.code = providerError.code
            detail.message = providerError.localizedDescription
        } else {
            detail.code = "NP_PROVIDER_SNAPSHOT_LOAD_FAILED"
            detail.message = "Provider 无法加载策略快照"
        }

        var request = Nonproxy_Provider_V1_AcknowledgeSnapshotRequest()
        request.context = try await session.requestContext()
        request.snapshotVersion = snapshot.metadata.snapshotVersion
        request.contentHash = snapshot.metadata.contentHash
        request.accepted = false
        request.error = detail
        _ = try await client.acknowledgeSnapshot(
            request: ClientRequest(message: request)
        )
    }

    private func reportReady<Transport: ClientTransport>(
        version: UInt64,
        metrics: ProviderHealthMetrics,
        using client: Nonproxy_Provider_V1_ProviderService.Client<Transport>
    ) async throws {
        var request = Nonproxy_Provider_V1_ReportHealthRequest()
        request.context = try await session.requestContext()
        request.state = .ready
        request.activeSnapshotVersion = version
        request.activeFlowCount = metrics.activeFlowCount
        request.queuedBytes = metrics.queuedBytes
        _ = try await client.reportHealth(
            request: ClientRequest(message: request)
        )
    }

    private func secureRandomBytes(count: Int) -> Data {
        var generator = SystemRandomNumberGenerator()
        return Data((0 ..< count).map { _ in
            UInt8.random(in: UInt8.min ... UInt8.max, using: &generator)
        })
    }
}
