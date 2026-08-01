import Foundation
import Network
import NonProxyProviderContracts

enum SnapshotContentValidator {
    private static let maximumIdentifierBytes = 128
    private static let maximumDisplayNameBytes = 128
    private static let maximumIdentityBytes = 512
    private static let maximumDomainBytes = 253
    private static let maximumDomainLabelBytes = 63

    static func validatePolicy(
        _ policy: Nonproxy_Policy_V1_Policy
    ) throws {
        guard isIdentifier(policy.id),
              isDisplayName(policy.displayName),
              policy.revision > 0,
              validSource(policy.sourceKind),
              validOrigin(policy.origin, for: policy.sourceKind)
        else {
            throw ProviderError.invalidSnapshot("策略基础字段或来源约束无效")
        }
        try validateMatcher(policy.match, source: policy.sourceKind)
        try validateDecision(policy.decision, availableOutbounds: nil)
    }

    static func validateNetworkProfile(
        _ profile: Nonproxy_Policy_V1_NetworkProfileBinding
    ) throws {
        guard isIdentifier(profile.id) else {
            throw ProviderError.invalidSnapshot("网络配置档基础字段无效")
        }
        switch profile.fingerprintKind {
        case .wifiSsidSha256, .defaultGatewaySha256:
            guard profile.fingerprintValue.utf8.count == 64,
                  profile.fingerprintValue.utf8.allSatisfy({
                      $0.isNumber || (97 ... 102).contains($0)
                  })
            else {
                throw ProviderError.invalidSnapshot("网络配置档哈希指纹无效")
            }
        case .interfaceClass:
            guard ["wifi", "ethernet", "cellular", "other"]
                .contains(profile.fingerprintValue)
            else {
                throw ProviderError.invalidSnapshot("网络配置档接口类型无效")
            }
        default:
            throw ProviderError.invalidSnapshot("网络配置档指纹类型无效")
        }
    }

    static func validateDecision(
        _ decision: Nonproxy_Policy_V1_DecisionSpec,
        availableOutbounds: Set<String>?
    ) throws {
        guard decision.failureMode == .closed || decision.failureMode == .open
        else {
            throw ProviderError.invalidSnapshot("策略失败模式无效")
        }
        switch decision.action {
        case .proxy:
            guard isIdentifier(decision.outboundID),
                  availableOutbounds?.contains(decision.outboundID) ?? true
            else {
                throw ProviderError.invalidSnapshot("代理决策缺少有效出口")
            }
        case .direct, .block:
            guard decision.outboundID.isEmpty else {
                throw ProviderError.invalidSnapshot("非代理决策不能绑定出口")
            }
        default:
            throw ProviderError.invalidSnapshot("策略动作无效")
        }
    }

    static func validateCapabilities(
        _ capabilities: Nonproxy_Policy_V1_CompileCapabilitySet,
        policies: [Nonproxy_Policy_V1_Policy],
        defaultDecision: Nonproxy_Policy_V1_DecisionSpec
    ) throws {
        let availableOutbounds = Set(capabilities.outbounds.map(\.outboundID))
        for policy in policies {
            let matcher = policy.match
            guard !matcher.hasApp || capabilities.appMatch,
                  !matcher.hasDomain || capabilities.domainMatch,
                  !matcher.hasCidr || capabilities.cidrMatch,
                  matcher.transports.allSatisfy(capabilities.transports.contains),
                  supportsCidrFamily(matcher, capabilities: capabilities)
            else {
                throw ProviderError.invalidSnapshot("策略使用了目标不支持的匹配能力")
            }
            try validateDecision(
                policy.decision,
                availableOutbounds: availableOutbounds
            )
            try validateOutboundCompatibility(
                policy.decision,
                matcher: matcher,
                capabilities: capabilities
            )
        }
        try validateOutboundCompatibility(
            defaultDecision,
            matcher: nil,
            capabilities: capabilities
        )
    }

    static func validateRuntimeOverride(
        _ runtimeOverride: Nonproxy_Policy_V1_RuntimeRoutingOverride,
        capabilities: Nonproxy_Policy_V1_CompileCapabilitySet
    ) throws {
        var decision = Nonproxy_Policy_V1_DecisionSpec()
        decision.failureMode = .closed
        switch runtimeOverride.mode {
        case .paused, .direct:
            guard runtimeOverride.outboundID.isEmpty else {
                throw ProviderError.invalidSnapshot("非代理运行态覆盖不能绑定出口")
            }
            guard runtimeOverride.mode == .direct else {
                return
            }
            decision.action = .direct
        case .proxy:
            decision.action = .proxy
            decision.outboundID = runtimeOverride.outboundID
        default:
            throw ProviderError.invalidSnapshot("运行态覆盖模式无效")
        }
        let availableOutbounds = Set(capabilities.outbounds.map(\.outboundID))
        try validateDecision(decision, availableOutbounds: availableOutbounds)
        try validateOutboundCompatibility(
            decision,
            matcher: nil,
            capabilities: capabilities
        )
    }

    static func isIdentifier(_ value: String) -> Bool {
        !value.isEmpty
            && value.utf8.count <= maximumIdentifierBytes
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
            && value.utf8.allSatisfy {
                $0.isASCII && (
                    $0.isLetter
                        || $0.isNumber
                        || $0 == Character(".").asciiValue
                        || $0 == Character("_").asciiValue
                        || $0 == Character(":").asciiValue
                        || $0 == Character("-").asciiValue
                )
            }
    }

    private static func validateMatcher(
        _ matcher: Nonproxy_Policy_V1_PolicyMatch,
        source: Nonproxy_Policy_V1_PolicySourceKind
    ) throws {
        let hasNonNetworkDimension = matcher.hasApp
            || matcher.hasDomain
            || matcher.hasCidr
            || !matcher.transports.isEmpty
            || !matcher.ports.isEmpty
        guard !(matcher.hasDomain && matcher.hasCidr),
              !(matcher.hasNetwork && hasNonNetworkDimension),
              validOrderedTransports(matcher.transports)
        else {
            throw ProviderError.invalidSnapshot("策略目标或传输协议约束无效")
        }
        if matcher.hasApp {
            try validateApp(matcher.app)
        }
        if matcher.hasDomain {
            try validateDomain(matcher.domain)
        }
        if matcher.hasCidr {
            try validateCidr(matcher.cidr)
        }
        if matcher.hasNetwork {
            guard isIdentifier(matcher.network.profileID) else {
                throw ProviderError.invalidSnapshot("网络配置档标识无效")
            }
        }
        try validatePorts(matcher.ports)
        guard validDimensions(matcher, for: source) else {
            throw ProviderError.invalidSnapshot("策略来源与匹配维度不一致")
        }
    }

    private static func validateApp(
        _ app: Nonproxy_Policy_V1_AppMatcher
    ) throws {
        guard app.platform == .macos || app.platform == .windows,
              isIdentityField(app.stableID, required: true),
              isIdentityField(app.signerID, required: false)
        else {
            throw ProviderError.invalidSnapshot("应用匹配身份无效")
        }
    }

    private static func validateDomain(
        _ domain: Nonproxy_Policy_V1_DomainMatcher
    ) throws {
        guard domain.kind == .exact
                || domain.kind == .suffix
                || domain.kind == .registrableDomain,
              domain.asciiPattern.utf8.count <= maximumDomainBytes,
              domain.asciiPattern == domain.asciiPattern.lowercased(),
              domain.asciiPattern.unicodeScalars.allSatisfy(\.isASCII),
              IPv4Address(domain.asciiPattern) == nil,
              IPv6Address(domain.asciiPattern) == nil
        else {
            throw ProviderError.invalidSnapshot("域名匹配模式无效")
        }
        let labels = domain.asciiPattern.split(separator: ".", omittingEmptySubsequences: false)
        guard !labels.isEmpty, labels.allSatisfy(validDomainLabel) else {
            throw ProviderError.invalidSnapshot("域名匹配标签无效")
        }
    }

    private static func validateCidr(
        _ cidr: Nonproxy_Policy_V1_CidrMatcher
    ) throws {
        if let address = IPv4Address(cidr.network) {
            guard cidr.prefixLength <= 32,
                  isCanonicalNetwork(address.rawValue, prefix: Int(cidr.prefixLength)),
                  String(describing: address) == cidr.network
            else {
                throw ProviderError.invalidSnapshot("IPv4 CIDR 网络前缀无效")
            }
            return
        }
        if let address = IPv6Address(cidr.network) {
            guard cidr.prefixLength <= 128,
                  isCanonicalNetwork(address.rawValue, prefix: Int(cidr.prefixLength)),
                  String(describing: address) == cidr.network
            else {
                throw ProviderError.invalidSnapshot("IPv6 CIDR 网络前缀无效")
            }
            return
        }
        throw ProviderError.invalidSnapshot("CIDR 网络地址无效")
    }

    private static func validatePorts(
        _ ports: [Nonproxy_Policy_V1_PortRange]
    ) throws {
        var previousLast: UInt32 = 0
        for (index, port) in ports.enumerated() {
            guard port.first > 0,
                  port.first <= port.last,
                  port.last <= UInt32(UInt16.max),
                  index == 0 || port.first > previousLast
            else {
                throw ProviderError.invalidSnapshot("策略端口范围无效或重叠")
            }
            previousLast = port.last
        }
    }

    private static func validDimensions(
        _ matcher: Nonproxy_Policy_V1_PolicyMatch,
        for source: Nonproxy_Policy_V1_PolicySourceKind
    ) -> Bool {
        let hasTransportOrPort = !matcher.transports.isEmpty || !matcher.ports.isEmpty
        switch source {
        case .appDestination:
            return matcher.hasApp && (matcher.hasDomain || matcher.hasCidr)
                && !matcher.hasNetwork
        case .app:
            return matcher.hasApp && !matcher.hasDomain && !matcher.hasCidr
                && !matcher.hasNetwork && !hasTransportOrPort
        case .site:
            return !matcher.hasApp && matcher.hasDomain && !matcher.hasCidr
                && !matcher.hasNetwork
        case .cidr:
            return !matcher.hasApp && !matcher.hasDomain && matcher.hasCidr
                && !matcher.hasNetwork
        case .network:
            return !matcher.hasApp && !matcher.hasDomain && !matcher.hasCidr
                && matcher.hasNetwork && !hasTransportOrPort
        case .system, .builtIn:
            return !matcher.hasNetwork
        case .adapter:
            return matcher.hasApp || matcher.hasDomain || matcher.hasCidr
        default:
            return false
        }
    }

    private static func validSource(
        _ source: Nonproxy_Policy_V1_PolicySourceKind
    ) -> Bool {
        (1 ... 8).contains(source.rawValue)
    }

    private static func validOrigin(
        _ origin: Nonproxy_Policy_V1_PolicyOrigin,
        for source: Nonproxy_Policy_V1_PolicySourceKind
    ) -> Bool {
        switch source {
        case .system:
            return origin == .system
        case .builtIn:
            return origin == .signedBuiltIn
        case .adapter:
            return origin == .adapter
        default:
            return origin == .user || origin == .subscription
        }
    }

    private static func validOrderedTransports(
        _ values: [Nonproxy_Common_V1_TransportProtocol]
    ) -> Bool {
        values.allSatisfy { $0 == .tcp || $0 == .udp }
            && zip(values, values.dropFirst()).allSatisfy {
                $0.rawValue < $1.rawValue
            }
    }

    private static func isDisplayName(_ value: String) -> Bool {
        !value.isEmpty
            && value.utf8.count <= maximumDisplayNameBytes
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private static func isIdentityField(
        _ value: String,
        required: Bool
    ) -> Bool {
        if value.isEmpty {
            return !required
        }
        return value.utf8.count <= maximumIdentityBytes
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private static func validDomainLabel(_ label: Substring) -> Bool {
        !label.isEmpty
            && label.utf8.count <= maximumDomainLabelBytes
            && label.first != "-"
            && label.last != "-"
            && label.utf8.allSatisfy {
                $0.isASCII && ($0.isLowercaseLetter || $0.isNumber || $0 == 45)
            }
    }

    private static func isCanonicalNetwork(
        _ address: Data,
        prefix: Int
    ) -> Bool {
        let fullBytes = prefix / 8
        let remainingBits = prefix % 8
        if remainingBits > 0 {
            let hostMask = UInt8.max >> remainingBits
            guard address[fullBytes] & hostMask == 0 else {
                return false
            }
        }
        return address.dropFirst(fullBytes + (remainingBits > 0 ? 1 : 0))
            .allSatisfy { $0 == 0 }
    }

    private static func supportsCidrFamily(
        _ matcher: Nonproxy_Policy_V1_PolicyMatch,
        capabilities: Nonproxy_Policy_V1_CompileCapabilitySet
    ) -> Bool {
        guard matcher.hasCidr else {
            return true
        }
        if IPv4Address(matcher.cidr.network) != nil {
            return capabilities.ipFamilies.contains(.ipv4)
        }
        if IPv6Address(matcher.cidr.network) != nil {
            return capabilities.ipFamilies.contains(.ipv6)
        }
        return false
    }

    private static func validateOutboundCompatibility(
        _ decision: Nonproxy_Policy_V1_DecisionSpec,
        matcher: Nonproxy_Policy_V1_PolicyMatch?,
        capabilities: Nonproxy_Policy_V1_CompileCapabilitySet
    ) throws {
        guard decision.action == .proxy else {
            return
        }
        guard let outbound = capabilities.outbounds.first(where: {
            $0.outboundID == decision.outboundID
        }) else {
            throw ProviderError.invalidSnapshot("代理决策引用了未知出口")
        }
        let transports = matcher.map(\.transports).flatMap {
            $0.isEmpty ? nil : $0
        } ?? capabilities.transports
        let families: [Nonproxy_Common_V1_IpFamily]
        if let matcher, matcher.hasCidr {
            families = IPv4Address(matcher.cidr.network) == nil ? [.ipv6] : [.ipv4]
        } else {
            families = capabilities.ipFamilies
        }
        guard transports.allSatisfy(outbound.transports.contains),
              families.allSatisfy(outbound.ipFamilies.contains)
        else {
            throw ProviderError.invalidSnapshot("代理出口能力无法满足策略")
        }
    }
}

private extension UInt8 {
    var isASCII: Bool {
        self < 128
    }

    var isLetter: Bool {
        (65 ... 90).contains(self) || (97 ... 122).contains(self)
    }

    var isLowercaseLetter: Bool {
        (97 ... 122).contains(self)
    }

    var isNumber: Bool {
        (48 ... 57).contains(self)
    }
}
