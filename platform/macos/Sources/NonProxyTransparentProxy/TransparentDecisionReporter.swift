import NonProxyProviderCore

enum TransparentDecisionReporter {
    static func report(
        runtime: TransparentProviderRuntime,
        observation: ProviderDecisionObservation,
        path: ProviderObservedPath,
        errorCode: String? = nil
    ) {
        guard let record = try? observation.record(
            path: path,
            errorCode: errorCode
        ) else {
            runtime.provider.decisions.recordUnreportable()
            return
        }
        runtime.provider.decisions.submit(record)
    }
}
