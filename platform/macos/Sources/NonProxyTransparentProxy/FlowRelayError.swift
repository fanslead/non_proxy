import NetworkExtension

enum FlowRelayError {
    static func abort(_ nonProxyCode: String) -> NSError {
        make(.aborted, nonProxyCode: nonProxyCode)
    }

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
