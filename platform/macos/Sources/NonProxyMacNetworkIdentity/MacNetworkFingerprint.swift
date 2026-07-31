import CryptoKit
import Foundation
import Network

public enum MacNetworkFingerprintKind: String, Codable, Sendable {
    case wifiSSIDHash = "wifi_ssid_sha256"
    case defaultGatewayHash = "default_gateway_sha256"
    case interfaceClass = "interface_class"
}

public struct MacNetworkFingerprint: Hashable, Codable, Sendable {
    public let kind: MacNetworkFingerprintKind
    public let value: String

    fileprivate init(kind: MacNetworkFingerprintKind, value: String) {
        self.kind = kind
        self.value = value
    }
}

public enum MacNetworkFingerprintFactory {
    public static func make(
        interfaceClass: String?,
        wifiSSID: String? = nil,
        defaultGateway: String? = nil
    ) -> [MacNetworkFingerprint] {
        [
            wifiSSID.flatMap(Self.wifiSSID),
            defaultGateway.flatMap { Self.defaultGateway($0) },
            interfaceClass.flatMap(Self.interfaceClass),
        ].compactMap { $0 }
    }

    public static func wifiSSID(
        _ value: String
    ) -> MacNetworkFingerprint? {
        guard !value.isEmpty else {
            return nil
        }
        return wifiSSIDData(Data(value.utf8))
    }

    public static func wifiSSIDData(
        _ value: Data
    ) -> MacNetworkFingerprint? {
        guard !value.isEmpty else {
            return nil
        }
        return MacNetworkFingerprint(
            kind: .wifiSSIDHash,
            value: sha256(value)
        )
    }

    public static func defaultGateway(
        _ value: String,
        hardwareAddress: String? = nil
    ) -> MacNetworkFingerprint? {
        guard let canonical = canonicalIPAddress(value) else {
            return nil
        }
        let family = IPv4Address(canonical) == nil ? "ipv6" : "ipv4"
        var identity = "\(family):\(canonical)"
        if let hardwareAddress = canonicalHardwareAddress(hardwareAddress) {
            identity += "|lladdr:\(hardwareAddress)"
        }
        return MacNetworkFingerprint(
            kind: .defaultGatewayHash,
            value: sha256(Data(identity.utf8))
        )
    }

    public static func interfaceClass(
        _ value: String
    ) -> MacNetworkFingerprint? {
        guard ["wifi", "ethernet", "cellular", "other"].contains(value) else {
            return nil
        }
        return MacNetworkFingerprint(kind: .interfaceClass, value: value)
    }

    public static func canonicalIPAddress(_ value: String) -> String? {
        if let address = IPv4Address(value) {
            return String(describing: address)
        }
        if let address = IPv6Address(value) {
            return String(describing: address)
        }
        return nil
    }

    public static func canonicalHardwareAddress(_ value: String?) -> String? {
        guard let value else {
            return nil
        }
        let normalized = value.lowercased()
        let octets = normalized.split(
            separator: ":",
            omittingEmptySubsequences: false
        )
        guard octets.count == 6,
              octets.allSatisfy({ octet in
                  octet.count == 2 && octet.utf8.allSatisfy {
                      (48 ... 57).contains($0) || (97 ... 102).contains($0)
                  }
              })
        else {
            return nil
        }
        return normalized
    }

    public static func name(for type: NWInterface.InterfaceType) -> String {
        switch type {
        case .wifi:
            "wifi"
        case .cellular:
            "cellular"
        case .wiredEthernet:
            "ethernet"
        default:
            "other"
        }
    }

    static func sha256(_ value: Data) -> String {
        SHA256.hash(data: value)
            .map { String(format: "%02x", $0) }
            .joined()
    }
}

struct MacNetworkInterfaceRank: Comparable {
    let isUsed: Bool
    let priority: Int
    let hasDefaultGateway: Bool
    let name: String

    static func < (
        lhs: MacNetworkInterfaceRank,
        rhs: MacNetworkInterfaceRank
    ) -> Bool {
        if lhs.isUsed != rhs.isUsed {
            return lhs.isUsed && !rhs.isUsed
        }
        if lhs.priority != rhs.priority {
            return lhs.priority < rhs.priority
        }
        if lhs.hasDefaultGateway != rhs.hasDefaultGateway {
            return lhs.hasDefaultGateway && !rhs.hasDefaultGateway
        }
        return lhs.name < rhs.name
    }
}
