import Foundation
import NetworkExtension

enum DNSFlowErrorFactory {
    static func make(
        _ code: NEAppProxyFlowError.Code,
        nonProxyCode: String
    ) -> Error {
        NSError(
            domain: NEAppProxyErrorDomain,
            code: code.rawValue,
            userInfo: [NSLocalizedDescriptionKey: nonProxyCode]
        )
    }
}
