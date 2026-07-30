import Foundation

public enum NativeRequestPayload: Equatable, Sendable {
    case hello
    case start(StartLearningPayload)
    case observe(ObserveLearningPayload)
    case list(SessionPayload)
    case stop(SessionPayload)
    case confirm(ConfirmLearningPayload)
}

public struct NativeRequest: Equatable, Sendable {
    public static let protocolVersion = 1

    public let requestID: String
    public let payload: NativeRequestPayload

    public init(
        requestID: String,
        payload: NativeRequestPayload
    ) {
        self.requestID = requestID
        self.payload = payload
    }
}

public struct StartLearningPayload: Codable, Equatable, Sendable {
    public let normalizedSite: String
    public let browserContextID: String
    public let durationMilliseconds: UInt64?

    public init(
        normalizedSite: String,
        browserContextID: String,
        durationMilliseconds: UInt64?
    ) {
        self.normalizedSite = normalizedSite
        self.browserContextID = browserContextID
        self.durationMilliseconds = durationMilliseconds
    }
}

public struct ObserveLearningPayload: Codable, Equatable, Sendable {
    public let sessionID: String
    public let observationID: String
    public let browserContextID: String
    public let kind: String
    public let normalizedDomain: String
    public let initiatorDomain: String?
    public let resourceType: String

    public init(
        sessionID: String,
        observationID: String,
        browserContextID: String,
        kind: String,
        normalizedDomain: String,
        initiatorDomain: String?,
        resourceType: String
    ) {
        self.sessionID = sessionID
        self.observationID = observationID
        self.browserContextID = browserContextID
        self.kind = kind
        self.normalizedDomain = normalizedDomain
        self.initiatorDomain = initiatorDomain
        self.resourceType = resourceType
    }
}

public struct SessionPayload: Codable, Equatable, Sendable {
    public let sessionID: String

    public init(sessionID: String) {
        self.sessionID = sessionID
    }
}

public struct ConfirmLearningPayload: Codable, Equatable, Sendable {
    public let sessionID: String
    public let confirmationID: String
    public let selectedDomains: [String]

    public init(
        sessionID: String,
        confirmationID: String,
        selectedDomains: [String]
    ) {
        self.sessionID = sessionID
        self.confirmationID = confirmationID
        self.selectedDomains = selectedDomains
    }
}

public struct NativeErrorPayload: Codable, Equatable, Sendable {
    public let code: String
    public let message: String
}

public enum NativeResponsePayload: Encodable, Equatable, Sendable {
    case hello(HelloResult)
    case started(StartLearningResult)
    case observed(ObservationResult)
    case candidates(CandidateListResult)
    case stopped(StopLearningResult)
    case confirmed(ConfirmLearningResult)
}

public struct NativeResponse: Encodable, Equatable, Sendable {
    public let protocolVersion: Int
    public let requestID: String
    public let ok: Bool
    public let payload: NativeResponsePayload?
    public let error: NativeErrorPayload?

    public static func success(
        requestID: String,
        payload: NativeResponsePayload
    ) -> Self {
        Self(
            protocolVersion: NativeRequest.protocolVersion,
            requestID: requestID,
            ok: true,
            payload: payload,
            error: nil
        )
    }

    public static func failure(
        requestID: String,
        error: NativeMessagingError
    ) -> Self {
        Self(
            protocolVersion: NativeRequest.protocolVersion,
            requestID: requestID,
            ok: false,
            payload: nil,
            error: NativeErrorPayload(
                code: error.code,
                message: error.localizedDescription
            )
        )
    }
}

public struct HelloResult: Codable, Equatable, Sendable {
    public let hostVersion: String
    public let capabilities: [String]

    public init(hostVersion: String, capabilities: [String]) {
        self.hostVersion = hostVersion
        self.capabilities = capabilities
    }
}

public struct StartLearningResult: Codable, Equatable, Sendable {
    public let sessionID: String
    public let expiresAtUnixMilliseconds: UInt64

    public init(
        sessionID: String,
        expiresAtUnixMilliseconds: UInt64
    ) {
        self.sessionID = sessionID
        self.expiresAtUnixMilliseconds = expiresAtUnixMilliseconds
    }
}

public struct ObservationResult: Codable, Equatable, Sendable {
    public let candidate: CandidateResult
    public let duplicate: Bool

    public init(candidate: CandidateResult, duplicate: Bool) {
        self.candidate = candidate
        self.duplicate = duplicate
    }
}

public struct CandidateListResult: Codable, Equatable, Sendable {
    public let session: SessionResult
    public let candidates: [CandidateResult]

    public init(
        session: SessionResult,
        candidates: [CandidateResult]
    ) {
        self.session = session
        self.candidates = candidates
    }
}

public struct StopLearningResult: Codable, Equatable, Sendable {
    public let session: SessionResult
    public let candidateCount: UInt32

    public init(session: SessionResult, candidateCount: UInt32) {
        self.session = session
        self.candidateCount = candidateCount
    }
}

public struct ConfirmLearningResult: Codable, Equatable, Sendable {
    public let policies: [ConfirmedPolicyResult]
    public let snapshotVersion: UInt64
    public let snapshotState: String
    public let replayed: Bool

    public init(
        policies: [ConfirmedPolicyResult],
        snapshotVersion: UInt64,
        snapshotState: String,
        replayed: Bool
    ) {
        self.policies = policies
        self.snapshotVersion = snapshotVersion
        self.snapshotState = snapshotState
        self.replayed = replayed
    }
}

public struct ConfirmedPolicyResult: Codable, Equatable, Sendable {
    public let normalizedDomain: String
    public let policyID: String

    public init(normalizedDomain: String, policyID: String) {
        self.normalizedDomain = normalizedDomain
        self.policyID = policyID
    }
}

public struct SessionResult: Codable, Equatable, Sendable {
    public let sessionID: String
    public let state: String
    public let normalizedSite: String?
    public let browserContextID: String?
    public let startedAtUnixMilliseconds: UInt64
    public let expiresAtUnixMilliseconds: UInt64
    public let stoppedAtUnixMilliseconds: UInt64?

    public init(
        sessionID: String,
        state: String,
        normalizedSite: String?,
        browserContextID: String?,
        startedAtUnixMilliseconds: UInt64,
        expiresAtUnixMilliseconds: UInt64,
        stoppedAtUnixMilliseconds: UInt64?
    ) {
        self.sessionID = sessionID
        self.state = state
        self.normalizedSite = normalizedSite
        self.browserContextID = browserContextID
        self.startedAtUnixMilliseconds = startedAtUnixMilliseconds
        self.expiresAtUnixMilliseconds = expiresAtUnixMilliseconds
        self.stoppedAtUnixMilliseconds = stoppedAtUnixMilliseconds
    }
}

public struct CandidateResult: Codable, Equatable, Sendable {
    public let normalizedDomain: String
    public let registrableDomain: String?
    public let kind: String
    public let confidenceMillis: UInt16
    public let requiresConfirmation: Bool
    public let evidenceCount: UInt32
    public let firstSeenAtUnixMilliseconds: UInt64
    public let lastSeenAtUnixMilliseconds: UInt64
    public let mainFrameCount: UInt32
    public let subresourceCount: UInt32
    public let redirectCount: UInt32

    public init(
        normalizedDomain: String,
        registrableDomain: String?,
        kind: String,
        confidenceMillis: UInt16,
        requiresConfirmation: Bool,
        evidenceCount: UInt32,
        firstSeenAtUnixMilliseconds: UInt64,
        lastSeenAtUnixMilliseconds: UInt64,
        mainFrameCount: UInt32,
        subresourceCount: UInt32,
        redirectCount: UInt32
    ) {
        self.normalizedDomain = normalizedDomain
        self.registrableDomain = registrableDomain
        self.kind = kind
        self.confidenceMillis = confidenceMillis
        self.requiresConfirmation = requiresConfirmation
        self.evidenceCount = evidenceCount
        self.firstSeenAtUnixMilliseconds =
            firstSeenAtUnixMilliseconds
        self.lastSeenAtUnixMilliseconds =
            lastSeenAtUnixMilliseconds
        self.mainFrameCount = mainFrameCount
        self.subresourceCount = subresourceCount
        self.redirectCount = redirectCount
    }
}

extension NativeResponsePayload {
    public func encode(to encoder: Encoder) throws {
        switch self {
        case .hello(let value):
            try value.encode(to: encoder)
        case .started(let value):
            try value.encode(to: encoder)
        case .observed(let value):
            try value.encode(to: encoder)
        case .candidates(let value):
            try value.encode(to: encoder)
        case .stopped(let value):
            try value.encode(to: encoder)
        case .confirmed(let value):
            try value.encode(to: encoder)
        }
    }
}
