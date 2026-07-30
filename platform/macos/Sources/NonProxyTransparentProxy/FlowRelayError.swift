import NetworkExtension

enum FlowRelayError {
    static func make(
        _ code: NEAppProxyFlowError.Code,
        nonProxyCode: String
    ) -> NSError {
        NSError(
            domain: NEAppProxyErrorDomain,
            code: code.rawValue,
            userInfo: ["NonProxyErrorCode": nonProxyCode]
        )
    }
}
