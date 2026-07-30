import NonProxyProviderContracts
import SwiftProtobuf

extension NativeLearningClient {
    func snapshotState(
        _ value: Nonproxy_Policy_V1_SnapshotState
    ) throws -> String {
        switch value {
        case .pendingAck:
            "pendingAck"
        case .active:
            "active"
        case .rejected:
            "rejected"
        case .rolledBack:
            "rolledBack"
        case .superseded:
            "superseded"
        case .draft:
            "draft"
        case .unspecified, .UNRECOGNIZED:
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的策略快照状态无效。"
            )
        }
    }

    func reject(
        _ error: Nonproxy_Common_V1_ErrorDetail?
    ) throws {
        guard let error else {
            return
        }
        throw NativeMessagingError.gatewayRejected(
            code: error.code.isEmpty
                ? "NP_NATIVE_GATEWAY_REJECTED"
                : error.code,
            message: error.message.isEmpty
                ? "gatewayd 拒绝了学习操作。"
                : error.message
        )
    }

    func unixMilliseconds(
        _ value: Google_Protobuf_Timestamp
    ) throws -> UInt64 {
        guard value.seconds >= 0,
              value.nanos >= 0,
              value.nanos < 1_000_000_000
        else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的时间戳无效。"
            )
        }
        let seconds = UInt64(value.seconds)
        let (milliseconds, overflow) =
            seconds.multipliedReportingOverflow(by: 1_000)
        guard !overflow else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的时间戳超出范围。"
            )
        }
        return milliseconds + UInt64(value.nanos / 1_000_000)
    }

    func observationKind(
        _ value: String
    ) throws -> Nonproxy_Control_V1_LearningObservationKind {
        switch value {
        case "mainFrame":
            .mainFrame
        case "subresource":
            .subresource
        case "redirect":
            .redirect
        default:
            throw NativeMessagingError.invalidMessage(
                "学习观测类型无效。"
            )
        }
    }

    func resourceType(
        _ value: String
    ) throws -> Nonproxy_Control_V1_LearningResourceType {
        switch value {
        case "mainFrame":
            .mainFrame
        case "subFrame":
            .subFrame
        case "script":
            .script
        case "styleSheet":
            .styleSheet
        case "image":
            .image
        case "font":
            .font
        case "media":
            .media
        case "xmlHttpRequest":
            .xmlHTTPRequest
        case "fetch":
            .fetch
        case "webSocket":
            .webSocket
        case "other":
            .other
        default:
            throw NativeMessagingError.invalidMessage(
                "学习资源类型无效。"
            )
        }
    }

    func session(
        _ value: Nonproxy_Control_V1_LearningSessionSummary
    ) throws -> SessionResult {
        guard value.hasStartedAt, value.hasExpiresAt else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的学习会话时间不完整。"
            )
        }
        let state: String
        switch value.state {
        case .active:
            state = "active"
        case .stopped:
            state = "stopped"
        case .expired:
            state = "expired"
        case .unspecified, .UNRECOGNIZED:
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的学习会话状态无效。"
            )
        }
        let site: String?
        switch value.subject {
        case .normalizedSite(let value):
            site = value
        case .app, nil:
            site = nil
        }
        return SessionResult(
            sessionID: value.sessionID,
            state: state,
            normalizedSite: site,
            browserContextID: value.browserContextID.isEmpty
                ? nil
                : value.browserContextID,
            startedAtUnixMilliseconds: try unixMilliseconds(
                value.startedAt
            ),
            expiresAtUnixMilliseconds: try unixMilliseconds(
                value.expiresAt
            ),
            stoppedAtUnixMilliseconds: value.hasStoppedAt
                ? try unixMilliseconds(value.stoppedAt)
                : nil
        )
    }

    func candidate(
        _ value: Nonproxy_Control_V1_LearningCandidate
    ) throws -> CandidateResult {
        guard value.hasFirstSeenAt, value.hasLastSeenAt else {
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的学习候选时间不完整。"
            )
        }
        let kind: String
        switch value.kind {
        case .requiredFirstParty:
            kind = "requiredFirstParty"
        case .likelyApi:
            kind = "likelyApi"
        case .likelyAuth:
            kind = "likelyAuth"
        case .likelyCdn:
            kind = "likelyCdn"
        case .thirdParty:
            kind = "thirdParty"
        case .unknown:
            kind = "unknown"
        case .unspecified, .UNRECOGNIZED:
            throw NativeMessagingError.runtimeUnavailable(
                "gatewayd 返回的学习候选类型无效。"
            )
        }
        let confidence = min(
            max(Int((value.confidence * 1_000).rounded()), 0),
            1_000
        )
        return CandidateResult(
            normalizedDomain: value.normalizedDomain,
            registrableDomain: value.registrableDomain.isEmpty
                ? nil
                : value.registrableDomain,
            kind: kind,
            confidenceMillis: UInt16(confidence),
            requiresConfirmation: value.requiresConfirmation,
            evidenceCount: value.evidenceCount,
            firstSeenAtUnixMilliseconds: try unixMilliseconds(
                value.firstSeenAt
            ),
            lastSeenAtUnixMilliseconds: try unixMilliseconds(
                value.lastSeenAt
            ),
            mainFrameCount: value.mainFrameCount,
            subresourceCount: value.subresourceCount,
            redirectCount: value.redirectCount
        )
    }
}
