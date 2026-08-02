import Foundation
import NonProxyProviderContracts
import SwiftProtobuf

public enum ProviderObservedPath: Sendable, Equatable {
    case decision
    case direct(interfaceName: String, failOpen: Bool)
    case proxy(outboundID: String)
}

public struct ProviderDecisionObservation: Sendable {
    public let flowID: String
    public let context: PolicyConnectionContext
    public let decision: PolicyDecision
    public let proxyTarget: ProviderProxyTarget?
    public let observedAt: Date
    public let decisionLatencyNanoseconds: UInt64

    public init(
        flowID: String,
        context: PolicyConnectionContext,
        decision: PolicyDecision,
        proxyTarget: ProviderProxyTarget? = nil,
        observedAt: Date,
        decisionLatencyNanoseconds: UInt64
    ) {
        self.flowID = flowID
        self.context = context
        self.decision = decision
        self.proxyTarget = proxyTarget
        self.observedAt = observedAt
        self.decisionLatencyNanoseconds = decisionLatencyNanoseconds
    }

    public func record(
        path: ProviderObservedPath,
        errorCode: String? = nil
    ) throws -> Nonproxy_Provider_V1_DecisionRecord {
        guard !flowID.isEmpty,
              decision.snapshotVersion > 0,
              decisionLatencyNanoseconds <= 60_000_000_000
        else {
            throw ProviderError.control("连接决策观测数据无效")
        }
        var record = Nonproxy_Provider_V1_DecisionRecord()
        record.context = try connectionContext()
        record.decision = policyDecision()
        record.evidence = try evidence(path: path, errorCode: errorCode)
        record.decisionLatency = duration(nanoseconds: decisionLatencyNanoseconds)
        if let errorCode {
            guard isStableErrorCode(errorCode) else {
                throw ProviderError.control("连接决策错误码无效")
            }
            var detail = Nonproxy_Common_V1_ErrorDetail()
            detail.code = errorCode
            record.error = detail
        }
        return record
    }

    private func connectionContext()
        throws -> Nonproxy_Provider_V1_ConnectionContext
    {
        var value = Nonproxy_Provider_V1_ConnectionContext()
        value.flowID = flowID
        value.app = appIdentity()
        value.destination = try destination()
        value.networkProfileID = context.networkProfileID ?? ""
        value.observedAt = timestamp(date: observedAt)
        return value
    }

    private func appIdentity() -> Nonproxy_Common_V1_AppIdentity {
        var value = Nonproxy_Common_V1_AppIdentity()
        value.platform = .macos
        value.stableID = context.app.stableID
        value.signerID = context.app.signerID ?? ""
        value.parentStableID = context.app.parentStableID ?? ""
        value.helperGroupID = context.app.helperGroupID ?? ""
        return value
    }

    private func destination() throws -> Nonproxy_Common_V1_Destination {
        guard context.destination.port > 0 else {
            throw ProviderError.control("连接决策目标端口无效")
        }
        var value = Nonproxy_Common_V1_Destination()
        value.hostname = context.destination.normalizedDomain ?? ""
        value.normalizedDomain = context.destination.normalizedDomain ?? ""
        value.ipAddress = context.destination.ipAddress ?? ""
        value.port = UInt32(context.destination.port)
        value.transport = context.destination.transport
        if let address = context.destination.ipAddress {
            value.ipFamily = address.contains(":") ? .ipv6 : .ipv4
        }
        return value
    }

    private func policyDecision() -> Nonproxy_Policy_V1_Decision {
        var value = Nonproxy_Policy_V1_Decision()
        value.result = decision.result
        value.matchedPolicyID = decision.matchedPolicyID ?? ""
        value.matchedRuleID = decision.matchedRuleID ?? ""
        value.snapshotVersion = decision.snapshotVersion
        value.reasonCode = decision.reasonCode
        return value
    }

    private func evidence(
        path: ProviderObservedPath,
        errorCode: String?
    ) throws -> Nonproxy_Provider_V1_DecisionEvidence {
        var value = Nonproxy_Provider_V1_DecisionEvidence()
        switch path {
        case .decision:
            guard errorCode?.isEmpty != true else {
                throw ProviderError.control("连接决策错误码无效")
            }
            value.level = .decision
        case .direct(let interfaceName, let failOpen):
            guard !interfaceName.isEmpty,
                  (!failOpen && decision.result.action == .direct)
                      || (
                          failOpen
                              && decision.result.action == .proxy
                              && decision.result.failureMode == .open
                              && errorCode != nil
                      )
            else {
                throw ProviderError.control("直连路径证据无效")
            }
            value.level = .path
            value.interfaceName = interfaceName
            value.failOpenDirect = failOpen
        case .proxy(let outboundID):
            guard !outboundID.isEmpty,
                  decision.result.action == .proxy,
                  proxyTarget?.matches(decision: decision) == true,
                  proxyTarget?.accepts(selectedOutboundID: outboundID) == true,
                  errorCode == nil
            else {
                throw ProviderError.control("代理路径证据无效")
            }
            value.level = .path
            value.outboundID = outboundID
        }
        return value
    }
}

private func timestamp(date: Date) -> Google_Protobuf_Timestamp {
    let interval = max(0, date.timeIntervalSince1970)
    let seconds = floor(interval)
    var value = Google_Protobuf_Timestamp()
    value.seconds = Int64(seconds)
    value.nanos = Int32((interval - seconds) * 1_000_000_000)
    return value
}

private func duration(nanoseconds: UInt64) -> Google_Protobuf_Duration {
    var value = Google_Protobuf_Duration()
    value.seconds = Int64(nanoseconds / 1_000_000_000)
    value.nanos = Int32(nanoseconds % 1_000_000_000)
    return value
}

private func isStableErrorCode(_ value: String) -> Bool {
    value.hasPrefix("NP_")
        && value.count <= 128
        && value.utf8.allSatisfy {
            $0 <= 127
                && ($0.isUppercaseASCII || $0.isDigitASCII || $0 == 95)
        }
}

private extension UInt8 {
    var isUppercaseASCII: Bool {
        (65 ... 90).contains(self)
    }

    var isDigitASCII: Bool {
        (48 ... 57).contains(self)
    }
}
