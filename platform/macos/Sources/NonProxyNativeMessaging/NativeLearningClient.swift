import Foundation
import GRPCCore
import NonProxyProviderContracts
import SwiftProtobuf

public final class NativeLearningClient: NativeLearningServing, Sendable {
    private let capability: Data
    private let connection: NativeControlConnection

    public init(configuration: NativeHostRuntimeConfiguration) throws {
        try configuration.validateControlSocket()
        self.capability = try configuration.readControlCapability()
        self.connection = try NativeControlConnection(
            socketPath: configuration.controlSocket.path
        )
    }

    public func start(
        _ payload: StartLearningPayload
    ) async throws -> StartLearningResult {
        var request = Nonproxy_Control_V1_StartLearningSessionRequest()
        request.context = operationContext(prefix: "native-start")
        request.kind = .site
        request.normalizedSite = payload.normalizedSite
        request.browserContextID = payload.browserContextID
        if let duration = payload.durationMilliseconds {
            guard duration <= UInt64(Int64.max) else {
                throw NativeMessagingError.invalidMessage(
                    "学习时长超出范围。"
                )
            }
            var value = Google_Protobuf_Duration()
            value.seconds = Int64(duration / 1_000)
            value.nanos = Int32(duration % 1_000) * 1_000_000
            request.duration = value
        }
        let authenticatedRequest = request
        let response: Nonproxy_Control_V1_StartLearningSessionResponse =
            try await connection.perform { client in
                try await client.startLearningSession(
                    request: ClientRequest(message: authenticatedRequest),
                    options: Self.callOptions
                )
            }
        try reject(response.hasError ? response.error : nil)
        guard !response.sessionID.isEmpty, response.hasExpiresAt else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的学习会话不完整。"
            )
        }
        return StartLearningResult(
            sessionID: response.sessionID,
            expiresAtUnixMilliseconds: try unixMilliseconds(
                response.expiresAt
            )
        )
    }

    public func observe(
        _ payload: ObserveLearningPayload
    ) async throws -> ObservationResult {
        var request = Nonproxy_Control_V1_RecordLearningObservationRequest()
        request.context = operationContext(prefix: "native-observe")
        request.sessionID = payload.sessionID
        request.observationID = payload.observationID
        request.browserContextID = payload.browserContextID
        request.kind = try observationKind(payload.kind)
        request.normalizedDomain = payload.normalizedDomain
        request.initiatorDomain = payload.initiatorDomain ?? ""
        request.resourceType = try resourceType(payload.resourceType)
        let authenticatedRequest = request
        let response: Nonproxy_Control_V1_RecordLearningObservationResponse =
            try await connection.perform { client in
                try await client.recordLearningObservation(
                    request: ClientRequest(message: authenticatedRequest),
                    options: Self.callOptions
                )
            }
        try reject(response.hasError ? response.error : nil)
        guard response.hasCandidate else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 未返回学习候选。"
            )
        }
        return ObservationResult(
            candidate: try candidate(response.candidate),
            duplicate: response.duplicate
        )
    }

    public func list(
        _ payload: SessionPayload
    ) async throws -> CandidateListResult {
        var request = Nonproxy_Control_V1_ListLearningCandidatesRequest()
        request.context = operationContext(prefix: "native-list")
        request.sessionID = payload.sessionID
        let authenticatedRequest = request
        let response: Nonproxy_Control_V1_ListLearningCandidatesResponse =
            try await connection.perform { client in
                try await client.listLearningCandidates(
                    request: ClientRequest(message: authenticatedRequest),
                    options: Self.callOptions
                )
            }
        try reject(response.hasError ? response.error : nil)
        guard response.hasSession else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 未返回学习会话。"
            )
        }
        return CandidateListResult(
            session: try session(response.session),
            candidates: try response.candidates.map(candidate)
        )
    }

    public func stop(
        _ payload: SessionPayload
    ) async throws -> StopLearningResult {
        var request = Nonproxy_Control_V1_StopLearningSessionRequest()
        request.context = operationContext(prefix: "native-stop")
        request.sessionID = payload.sessionID
        let authenticatedRequest = request
        let response: Nonproxy_Control_V1_StopLearningSessionResponse =
            try await connection.perform { client in
                try await client.stopLearningSession(
                    request: ClientRequest(message: authenticatedRequest),
                    options: Self.callOptions
                )
            }
        try reject(response.hasError ? response.error : nil)
        guard response.hasSession else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 未返回停止后的学习会话。"
            )
        }
        return StopLearningResult(
            session: try session(response.session),
            candidateCount: response.candidateCount
        )
    }

    public func confirm(
        _ payload: ConfirmLearningPayload
    ) async throws -> ConfirmLearningResult {
        var request =
            Nonproxy_Control_V1_ConfirmLearningCandidatesRequest()
        request.context = operationContext(prefix: "native-confirm")
        request.sessionID = payload.sessionID
        request.confirmationID = payload.confirmationID
        request.selectedDomains = payload.selectedDomains
        let authenticatedRequest = request
        let response:
            Nonproxy_Control_V1_ConfirmLearningCandidatesResponse =
            try await connection.perform { client in
                try await client.confirmLearningCandidates(
                    request: ClientRequest(
                        message: authenticatedRequest
                    ),
                    options: Self.callOptions
                )
            }
        try reject(response.hasError ? response.error : nil)
        guard response.hasSnapshot else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 未返回确认后的策略快照。"
            )
        }
        return ConfirmLearningResult(
            policies: response.policies.map {
                ConfirmedPolicyResult(
                    normalizedDomain: $0.normalizedDomain,
                    policyID: $0.policyID
                )
            },
            snapshotVersion: response.snapshot.snapshotVersion,
            snapshotState: try snapshotState(
                response.snapshot.state
            ),
            replayed: response.replayed
        )
    }

    public func shutdown() {
        connection.shutdown()
    }

    private func operationContext(
        prefix: String
    ) -> Nonproxy_Control_V1_OperationContext {
        var generator = SystemRandomNumberGenerator()
        let suffix = (0 ..< 16).map { _ in
            String(
                format: "%02x",
                UInt8.random(
                    in: UInt8.min ... UInt8.max,
                    using: &generator
                )
            )
        }.joined()
        var context = Nonproxy_Control_V1_OperationContext()
        context.operationID = "\(prefix)-\(suffix)"
        context.sessionCapabilityToken = capability
        return context
    }

    private static var callOptions: CallOptions {
        var options = CallOptions.defaults
        options.timeout = .seconds(10)
        options.waitForReady = true
        options.maxRequestMessageBytes = 128 * 1_024
        options.maxResponseMessageBytes = 512 * 1_024
        return options
    }
}
