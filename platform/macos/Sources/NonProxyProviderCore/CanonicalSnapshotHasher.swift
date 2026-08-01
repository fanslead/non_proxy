import CryptoKit
import Foundation
import Network
import NonProxyProviderContracts
import SwiftProtobuf

enum CanonicalSnapshotHasher {
    static func hash(
        schemaVersion: UInt32,
        payload: Nonproxy_Policy_V1_CompiledPolicyPayload
    ) throws -> Data {
        var bytes = CanonicalBytes()
        bytes.appendUInt32(schemaVersion)
        try appendDecision(payload.defaultDecision, to: &bytes)

        bytes.appendUInt64(UInt64(payload.policies.count))
        for policy in payload.policies {
            bytes.appendString(policy.id)
            bytes.appendByte(try sourceCode(policy.sourceKind))
            bytes.appendInt32(policy.priority)

            var matcherBytes = CanonicalBytes()
            try appendMatcher(policy.match, to: &matcherBytes)
            bytes.appendBytes(matcherBytes.data)
            try appendDecision(policy.decision, to: &bytes)
        }

        let outbounds = payload.capabilities.outbounds.sorted {
            $0.outboundID < $1.outboundID
        }
        bytes.appendUInt64(UInt64(outbounds.count))
        for outbound in outbounds {
            bytes.appendString(outbound.outboundID)
            bytes.appendByte(outbound.transports.contains(.tcp) ? 1 : 0)
            bytes.appendByte(outbound.transports.contains(.udp) ? 1 : 0)
            bytes.appendByte(outbound.ipFamilies.contains(.ipv4) ? 1 : 0)
            bytes.appendByte(outbound.ipFamilies.contains(.ipv6) ? 1 : 0)
        }
        if payload.formatVersion >= SnapshotValidator.networkProfilePayloadVersion {
            let networkProfiles = payload.networkProfiles.sorted { $0.id < $1.id }
            bytes.appendUInt64(UInt64(networkProfiles.count))
            for profile in networkProfiles {
                bytes.appendString(profile.id)
                bytes.appendByte(try fingerprintCode(profile.fingerprintKind))
                bytes.appendString(profile.fingerprintValue)
            }
        }
        if payload.formatVersion >= SnapshotValidator.payloadVersion {
            if payload.hasRuntimeOverride {
                bytes.appendByte(1)
                bytes.appendByte(try runtimeOverrideCode(payload.runtimeOverride.mode))
                bytes.appendOptionalString(optional(payload.runtimeOverride.outboundID))
                bytes.appendUInt64(
                    try unixMilliseconds(payload.runtimeOverride.expiresAt)
                )
            } else {
                bytes.appendByte(0)
            }
        }
        return Data(SHA256.hash(data: bytes.data))
    }

    private static func appendMatcher(
        _ matcher: Nonproxy_Policy_V1_PolicyMatch,
        to bytes: inout CanonicalBytes
    ) throws {
        if matcher.hasApp {
            bytes.appendByte(1)
            bytes.appendByte(try platformCode(matcher.app.platform))
            bytes.appendString(matcher.app.stableID)
            bytes.appendOptionalString(optional(matcher.app.signerID))
            bytes.appendByte(matcher.app.includeHelpers ? 1 : 0)
        } else {
            bytes.appendByte(0)
        }

        if matcher.hasDomain {
            bytes.appendByte(1)
            bytes.appendByte(try domainCode(matcher.domain.kind))
            bytes.appendString(matcher.domain.asciiPattern)
        } else {
            bytes.appendByte(0)
        }

        if matcher.hasCidr {
            bytes.appendByte(1)
            try appendAddress(matcher.cidr, to: &bytes)
        } else {
            bytes.appendByte(0)
        }

        if matcher.hasNetwork {
            bytes.appendByte(1)
            bytes.appendString(matcher.network.profileID)
        } else {
            bytes.appendByte(0)
        }

        bytes.appendUInt64(UInt64(matcher.transports.count))
        for transport in matcher.transports {
            bytes.appendByte(try transportCode(transport))
        }
        bytes.appendUInt64(UInt64(matcher.ports.count))
        for port in matcher.ports {
            guard port.first <= UInt16.max, port.last <= UInt16.max else {
                throw ProviderError.invalidSnapshot("策略端口超出有效范围")
            }
            bytes.appendUInt16(UInt16(port.first))
            bytes.appendUInt16(UInt16(port.last))
        }
    }

    private static func appendAddress(
        _ cidr: Nonproxy_Policy_V1_CidrMatcher,
        to bytes: inout CanonicalBytes
    ) throws {
        if let address = IPv4Address(cidr.network) {
            bytes.appendByte(4)
            bytes.appendData(address.rawValue)
        } else if let address = IPv6Address(cidr.network) {
            bytes.appendByte(6)
            bytes.appendData(address.rawValue)
        } else {
            throw ProviderError.invalidSnapshot("策略包含无效的 CIDR 地址")
        }
        guard cidr.prefixLength <= UInt8.max else {
            throw ProviderError.invalidSnapshot("策略 CIDR 前缀超出有效范围")
        }
        bytes.appendByte(UInt8(cidr.prefixLength))
    }

    private static func appendDecision(
        _ decision: Nonproxy_Policy_V1_DecisionSpec,
        to bytes: inout CanonicalBytes
    ) throws {
        switch decision.action {
        case .direct:
            bytes.appendByte(1)
        case .proxy:
            bytes.appendByte(2)
        case .block:
            bytes.appendByte(3)
        default:
            throw ProviderError.invalidSnapshot("策略动作无效")
        }
        bytes.appendOptionalString(optional(decision.outboundID))
        switch decision.failureMode {
        case .closed:
            bytes.appendByte(1)
        case .open:
            bytes.appendByte(2)
        default:
            throw ProviderError.invalidSnapshot("策略失败模式无效")
        }
    }

    private static func sourceCode(
        _ source: Nonproxy_Policy_V1_PolicySourceKind
    ) throws -> UInt8 {
        guard (1 ... 8).contains(source.rawValue) else {
            throw ProviderError.invalidSnapshot("策略来源类型无效")
        }
        return UInt8(source.rawValue)
    }

    private static func platformCode(
        _ platform: Nonproxy_Common_V1_Platform
    ) throws -> UInt8 {
        guard platform == .macos || platform == .windows else {
            throw ProviderError.invalidSnapshot("应用平台类型无效")
        }
        return UInt8(platform.rawValue)
    }

    private static func domainCode(
        _ kind: Nonproxy_Policy_V1_DomainMatchKind
    ) throws -> UInt8 {
        guard (1 ... 3).contains(kind.rawValue) else {
            throw ProviderError.invalidSnapshot("域名匹配类型无效")
        }
        return UInt8(kind.rawValue)
    }

    private static func transportCode(
        _ transport: Nonproxy_Common_V1_TransportProtocol
    ) throws -> UInt8 {
        guard transport == .tcp || transport == .udp else {
            throw ProviderError.invalidSnapshot("传输协议类型无效")
        }
        return UInt8(transport.rawValue)
    }

    private static func fingerprintCode(
        _ kind: Nonproxy_Policy_V1_NetworkFingerprintKind
    ) throws -> UInt8 {
        switch kind {
        case .wifiSsidSha256:
            1
        case .defaultGatewaySha256:
            2
        case .interfaceClass:
            3
        default:
            throw ProviderError.invalidSnapshot("网络配置档指纹类型无效")
        }
    }

    private static func runtimeOverrideCode(
        _ mode: Nonproxy_Policy_V1_RuntimeOverrideMode
    ) throws -> UInt8 {
        switch mode {
        case .paused:
            1
        case .direct:
            2
        case .proxy:
            3
        default:
            throw ProviderError.invalidSnapshot("运行态覆盖模式无效")
        }
    }

    private static func unixMilliseconds(
        _ timestamp: Google_Protobuf_Timestamp
    ) throws -> UInt64 {
        guard timestamp.seconds >= 0,
              timestamp.nanos >= 0,
              timestamp.nanos < 1_000_000_000,
              timestamp.nanos % 1_000_000 == 0,
              let seconds = UInt64(exactly: timestamp.seconds)
        else {
            throw ProviderError.invalidSnapshot("运行态覆盖到期时间无效")
        }
        let (milliseconds, multiplyOverflow) = seconds.multipliedReportingOverflow(by: 1_000)
        let (total, addOverflow) = milliseconds.addingReportingOverflow(
            UInt64(timestamp.nanos / 1_000_000)
        )
        guard !multiplyOverflow, !addOverflow, total > 0 else {
            throw ProviderError.invalidSnapshot("运行态覆盖到期时间无效")
        }
        return total
    }

    private static func optional(_ value: String) -> String? {
        value.isEmpty ? nil : value
    }
}
