import Foundation

struct ProxyTCPInboundWrite {
    let data: Data
    let acknowledgedBytes: Int
}

func sanitizedGatewayCode(_ payload: Data) -> String {
    guard payload.count <= 128,
          let value = String(data: payload, encoding: .utf8),
          value.hasPrefix("NP_"),
          value.utf8.allSatisfy({
              (48...57).contains($0)
                  || (65...90).contains($0)
                  || $0 == 95
          })
    else {
        return "NP_PROXY_GATEWAY_ERROR"
    }
    return value
}
