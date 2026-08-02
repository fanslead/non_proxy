import Foundation
import NonProxyProviderCore

struct ProxyTCPInboundWrite {
    let data: Data
    let acknowledgedBytes: Int
}

func sanitizedGatewayCode(_ payload: Data) -> String {
    NPF1PayloadCodec.decodeErrorCode(payload)
}
