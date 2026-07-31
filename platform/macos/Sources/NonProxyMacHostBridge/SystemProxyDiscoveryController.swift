import Foundation
import SystemConfiguration

@MainActor
struct SystemProxyDiscoveryController {
    func discover() throws -> [SystemProxyDescriptor] {
        guard let raw = SCDynamicStoreCopyProxies(nil) else {
            throw BridgeError(
                code: "NP_MAC_SYSTEM_PROXY_READ_FAILED",
                message: "macOS 没有返回当前系统代理设置。"
            )
        }
        guard let values = raw as? [String: Any] else {
            throw BridgeError(
                code: "NP_MAC_SYSTEM_PROXY_READ_FAILED",
                message: "macOS 返回了无法识别的系统代理设置。"
            )
        }
        return Self.discover(values: values)
    }

    static func discover(values: [String: Any]) -> [SystemProxyDescriptor] {
        let definitions = [
            Definition(
                enabledKey: "SOCKSEnable",
                hostKey: "SOCKSProxy",
                portKey: "SOCKSPort",
                suggestedID: "system-socks5",
                displayName: "系统 SOCKS5 代理",
                kind: "socks5"
            ),
            Definition(
                enabledKey: "HTTPEnable",
                hostKey: "HTTPProxy",
                portKey: "HTTPPort",
                suggestedID: "system-http",
                displayName: "系统 HTTP 代理",
                kind: "http_connect"
            ),
            Definition(
                enabledKey: "HTTPSEnable",
                hostKey: "HTTPSProxy",
                portKey: "HTTPSPort",
                suggestedID: "system-https",
                displayName: "系统 HTTPS CONNECT 代理",
                kind: "http_connect"
            ),
        ]
        var identities = Set<String>()
        return definitions.compactMap { definition in
            guard enabled(values[definition.enabledKey]),
                  let host = normalizedHost(values[definition.hostKey]),
                  let port = normalizedPort(values[definition.portKey])
            else {
                return nil
            }
            let identity = "\(definition.kind)|\(host.lowercased())|\(port)"
            guard identities.insert(identity).inserted else {
                return nil
            }
            return SystemProxyDescriptor(
                suggestedID: definition.suggestedID,
                displayName: definition.displayName,
                kind: definition.kind,
                host: host,
                port: port
            )
        }
    }

    private static func enabled(_ value: Any?) -> Bool {
        (value as? NSNumber)?.boolValue == true
    }

    private static func normalizedHost(_ value: Any?) -> String? {
        guard let value = value as? String else {
            return nil
        }
        let host = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !host.isEmpty,
              host.utf8.count <= 253,
              host.rangeOfCharacter(from: .controlCharacters) == nil,
              host.rangeOfCharacter(from: .whitespacesAndNewlines) == nil,
              !host.contains(where: { "@/?#[]".contains($0) })
        else {
            return nil
        }
        return host
    }

    private static func normalizedPort(_ value: Any?) -> UInt16? {
        guard let number = value as? NSNumber else {
            return nil
        }
        let port = number.int64Value
        guard port > 0, port <= Int64(UInt16.max) else {
            return nil
        }
        return UInt16(port)
    }

    private struct Definition {
        let enabledKey: String
        let hostKey: String
        let portKey: String
        let suggestedID: String
        let displayName: String
        let kind: String
    }
}
